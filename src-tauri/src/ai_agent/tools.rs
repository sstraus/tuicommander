use serde::Serialize;
use serde_json::{Value, json};
use std::io::Write;
use std::sync::Arc;

use super::safety::{KeyRisk, RegexSafetyChecker, SafeKey, SafetyVerdict};
use super::sandbox::FileSandbox;
use super::watcher;
use crate::state::AppState;

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
        Self {
            success: true,
            output: output.into(),
            needs_approval: false,
            approval_reason: None,
            approval_command: None,
        }
    }

    pub fn err(output: impl Into<String>) -> Self {
        Self {
            success: false,
            output: output.into(),
            needs_approval: false,
            approval_reason: None,
            approval_command: None,
        }
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
            "description": "Read terminal content. Returns JSON {screen, cursor, shell_state, awaiting_input, agent_intent?, agent_type?}. shell_state is 'busy' while the agent is working (a spinner like 'Caramelizing…' means busy, NOT idle) and 'idle' when it has stopped; awaiting_input is true when blocked on a user question. Pass since_cursor from a previous response to get only new lines (delta mode); omit for full viewport snapshot.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "PTY session ID" },
                    "lines": { "type": "integer", "description": "Max lines to return (default: 50, capped at 500)", "default": 50 },
                    "since_cursor": { "type": "integer", "description": "Cursor from a previous call — returns only new log lines since this position" }
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
            "description": "Get compact terminal context: shell state, CWD, git branch (from .git/HEAD, no subprocess), the last command's exit code, and agent type. Cheap — prefer this over running pwd/git for orientation.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "PTY session ID" }
                },
                "required": ["session_id"]
            }
        },
        {
            "name": "get_command_history",
            "description": "List recent commands run in this session, captured from OSC 133 shell-integration markers. Each entry has {command, cwd, exit_code, duration_ms, error_type, timestamp}, most-recent first. Use this instead of scraping the screen to see what ran and what failed.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "PTY session ID" },
                    "limit": { "type": "integer", "description": "Max entries (default: 20, capped at 200)", "default": 20 },
                    "errors_only": { "type": "boolean", "description": "Only return commands that errored", "default": false }
                },
                "required": ["session_id"]
            }
        },
        {
            "name": "explain_last_failure",
            "description": "Return the most recent failed command (non-zero exit or classified error) with its captured output, exit code, error type, and cwd. Answers 'why did my last command fail?' in one call. Returns {found:false} if nothing has failed.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "PTY session ID" }
                },
                "required": ["session_id"]
            }
        },
        {
            "name": "get_error_fixes",
            "description": "Return known error→fix correlations for this session: for each error_type, the commands that previously resolved it (a Success recorded shortly after the error). Use before retrying a known-failing operation.",
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
            "name": "search_code",
            "description": "Semantic BM25 search across all files in the repo. Returns ranked files with a snippet from the most relevant section. Use for discovering which files relate to a concept or feature. Follow up with read_file or search_files for exact matches.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Natural language or keyword query (e.g. 'authentication middleware', 'rate limit', 'file sandbox')" },
                    "limit": { "type": "integer", "description": "Max results to return (default 10, max 20)", "default": 10 }
                },
                "required": ["query"]
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
        },
        {
            "name": "search_tools",
            "description": "Discover upstream MCP tools (e.g. Jira, GitHub, Slack) registered with TUICommander. Returns tool names and descriptions. Use before calling call_tool to find the exact prefixed name (format: upstream__tool_name).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Optional substring filter on tool name or description" },
                    "limit": { "type": "integer", "description": "Max results to return (default 20)", "default": 20 }
                },
                "required": []
            }
        },
        {
            "name": "call_tool",
            "description": "Call an upstream MCP tool by its prefixed name (format: upstream__tool_name, e.g. jira__create_issue). Use search_tools to discover available tools and their argument schemas. Output truncated at 30K chars.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tool_name": { "type": "string", "description": "Prefixed tool name in upstream__tool_name format" },
                    "args": { "type": "object", "description": "Arguments to pass to the upstream tool" }
                },
                "required": ["tool_name"]
            }
        },
        {
            "name": "list_sessions",
            "description": "List all active PTY sessions with their id, name, shell state, and agent type. Use to find targets for cross-session orchestration (send_input, read_screen to other sessions).",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        },
        {
            "name": "spawn_session",
            "description": "Create a new PTY terminal session. Returns the session_id of the new tab. Use to launch new agents (e.g. Claude Code) that you can then orchestrate via send_input/read_screen.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "cwd": { "type": "string", "description": "Working directory for the new session (default: current repo root)" },
                    "name": { "type": "string", "description": "Display name for the new tab" }
                },
                "required": []
            }
        },
        {
            "name": "get_agent_status",
            "description": "Query the status of an agent loop running in another session. Returns state (running/paused/completed/cancelled/error) or null if no agent is active.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target_session_id": { "type": "string", "description": "Session ID to query" }
                },
                "required": ["target_session_id"]
            }
        },
        {
            "name": "drive_agent",
            "description": "Atomic send→wait→read. Sends command to session, waits for idle/pattern, returns screen + structured state. Omit command to just wait+read. Returns cursor for delta reads.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Target PTY session ID" },
                    "command": { "type": "string", "description": "Text to send (omit to just wait+read without sending)" },
                    "timeout_ms": { "type": "integer", "description": "Max wait time in ms (default: 30000)", "default": 30000 },
                    "wait_pattern": { "type": "string", "description": "Optional regex — return early when matched instead of waiting for idle" },
                    "lines": { "type": "integer", "description": "Max screen lines to return (default: 80)", "default": 80 },
                    "since_cursor": { "type": "integer", "description": "Cursor from a previous call — returns only new log lines since this position instead of the screen snapshot" }
                },
                "required": ["session_id"]
            }
        },
        {
            "name": "schedule_task",
            "description": "Schedule a recurring or one-shot agent task. The agent will run automatically at the specified interval.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "goal": { "type": "string", "description": "Goal or task description for the scheduled agent (max 500 chars)" },
                    "interval_minutes": { "type": "integer", "description": "Run interval in minutes (minimum 5)", "minimum": 5 },
                    "one_shot": { "type": "boolean", "description": "If true, run once and disable. Default false.", "default": false }
                },
                "required": ["goal", "interval_minutes"]
            }
        },
        {
            "name": "list_schedules",
            "description": "List all scheduled agent tasks with their id, goal, interval, and enabled status.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "cancel_schedule",
            "description": "Remove a scheduled agent task by id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Job id to cancel" }
                },
                "required": ["id"]
            }
        },
        {
            "name": "watch_for",
            "description": "Arm a reactive watch on THIS session: when the trigger fires, a fresh autonomous agent conversation runs your instructions. Requires user approval to arm. Bounded by max_fires and cooldown so it cannot loop unchecked.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "trigger": {
                        "type": "object",
                        "description": "Trigger spec. One of: {\"type\":\"command_done\",\"on_failure_only\":true} | {\"type\":\"error\"} | {\"type\":\"idle\"} | {\"type\":\"busy\"} | {\"type\":\"question\",\"confident_only\":true} | {\"type\":\"unseen\"} | {\"type\":\"pattern\",\"regex\":\"...\"}",
                        "properties": {
                            "type": { "type": "string", "enum": ["idle", "busy", "command_done", "question", "error", "unseen", "pattern"] }
                        },
                        "required": ["type"]
                    },
                    "instructions": { "type": "string", "description": "What the agent should do when the watch fires (max 8192 chars)" },
                    "name": { "type": "string", "description": "Optional human-readable label for the watch" },
                    "max_fires": { "type": "integer", "description": "Max times this watch may fire before auto-stopping (default 3, min 1)", "minimum": 1 },
                    "cooldown_secs": { "type": "integer", "description": "Minimum seconds between fires (default 10, min 5)", "minimum": 5 }
                },
                "required": ["trigger", "instructions"]
            }
        },
        {
            "name": "list_watches",
            "description": "List reactive watches armed on THIS session, with id, name, trigger, status, and fire count.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "cancel_watch",
            "description": "Cancel a reactive watch on THIS session by id (from list_watches).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "watch_id": { "type": "string", "description": "Watch id to cancel" }
                },
                "required": ["watch_id"]
            }
        },
        {
            "name": "search_scrollback",
            "description": "Regex-search a session's terminal scrollback (visible screen + history). Returns matching lines with line_index and match offsets; secrets are redacted.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Session to search" },
                    "query": { "type": "string", "description": "Regex pattern (alacritty syntax, max 1024 chars)" },
                    "limit": { "type": "integer", "description": "Max matches to return (default 50, max 500)" }
                },
                "required": ["session_id", "query"]
            }
        },
        {
            "name": "get_hyperlinks",
            "description": "List OSC 8 hyperlinks on a session's active screen (e.g. file:// or https:// links emitted by tools). Returns line_index, column span, and URI per link; secrets are redacted.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Session to inspect" }
                },
                "required": ["session_id"]
            }
        },
        {
            "name": "get_semantic_zones",
            "description": "Extract OSC 133 semantic zones (prompt / input / output) from a session's active screen, grouping contiguous same-type cells. Returns kind, line span, and text per zone; secrets are redacted.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Session to inspect" }
                },
                "required": ["session_id"]
            }
        }
    ])
}

// ── Secret redaction ──────────────────────────────────────────

/// Redact known secret patterns from terminal output.
///
/// The bare-hex catch-all (`\b[0-9a-fA-F]{40,}\b`) used to redact every
/// git SHA-1, lockfile hash, and package checksum it saw. Now hex is only
/// redacted when preceded by a secret-context word (`token=`, `secret:`,
/// `password=`, etc.), so `git log/show/diff`, `Cargo.lock`, and
/// `package-lock.json` round-trip verbatim. (#1369-f051)
pub fn redact_secrets(text: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;

    static PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
        vec![
            // API keys / tokens
            (Regex::new(r"sk-[A-Za-z0-9_-]{20,}").unwrap(), "[REDACTED]"),
            (Regex::new(r"AKIA[A-Z0-9]{16}").unwrap(), "[REDACTED]"),
            (Regex::new(r"ghp_[A-Za-z0-9]{36,}").unwrap(), "[REDACTED]"),
            (Regex::new(r"gho_[A-Za-z0-9]{36,}").unwrap(), "[REDACTED]"),
            (Regex::new(r"github_pat_[A-Za-z0-9_]{82,}").unwrap(), "[REDACTED]"),
            (Regex::new(r"xoxb-[A-Za-z0-9\-]+").unwrap(), "[REDACTED]"),
            (Regex::new(r"ya29\.[A-Za-z0-9_-]+").unwrap(), "[REDACTED]"),
            // PEM private keys (header + body)
            (Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----").unwrap(), "[REDACTED]"),
            (Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----").unwrap(), "[REDACTED]"),
            // Bearer tokens
            (Regex::new(r"Bearer\s+[A-Za-z0-9_\-.]+").unwrap(), "[REDACTED]"),
            // Database URLs with credentials
            (Regex::new(r"(?i)(postgres|mysql|mongodb|redis)://[^\s@]+@[^\s]+").unwrap(), "[REDACTED]"),
            // Generic DATABASE_URL value
            (Regex::new(r"DATABASE_URL=[^\s]+").unwrap(), "[REDACTED]"),
            // Context-bound hex tokens — preserve git SHAs / lockfile checksums.
            // Only redact when preceded by a secret-context word + separator.
            (
                Regex::new(
                    r"(?i)((?:token|secret|api[_-]?key|password|passwd|authorization|bearer|session[_-]?id|credential|signature)[\s]*[:=][\s]*)[0-9a-fA-F]{40,}\b",
                )
                .unwrap(),
                "${1}[REDACTED]",
            ),
            // .env key=value: variable names that contain secret-context words.
            // Matches STRIPE_SECRET_KEY=…, DB_PASSWORD=…, MY_SECRET_TOKEN=… etc.
            // Does NOT match DATABASE_HOST, PATH, PORT.
            (
                Regex::new(
                    r"(?i)([A-Z_0-9]*(?:SECRET|PASSWORD|PASSWD|TOKEN|API_KEY|PRIVATE_KEY|CREDENTIAL)[A-Z_0-9]*\s*=\s*)\S+",
                )
                .unwrap(),
                "${1}[REDACTED]",
            ),
            // High-entropy values for variable names ending in _KEY, _SECRET, _TOKEN.
            // Catches STRIPE_API_KEY=rk_live_... even without 'SECRET' in the name.
            (
                Regex::new(
                    r"(?i)([A-Z_0-9]+_(?:KEY|SECRET|TOKEN)\s*=\s*)[A-Za-z0-9+/=_\-]{20,}",
                )
                .unwrap(),
                "${1}[REDACTED]",
            ),
        ]
    });

    let mut result = text.to_owned();
    for (pattern, replacement) in PATTERNS.iter() {
        result = pattern.replace_all(&result, *replacement).to_string();
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
fn safe_pty_write(state: &AppState, session_id: &str, command: &str) -> Result<(), String> {
    let entry = state
        .sessions
        .get(session_id)
        .ok_or_else(|| format!("Session not found: {session_id}"))?;
    let mut session = entry.lock();

    // Write 1: Ctrl-U + command text
    let payload = format!("\x15{command}");
    session
        .writer
        .write_all(payload.as_bytes())
        .map_err(|e| format!("PTY write failed: {e}"))?;
    session
        .writer
        .flush()
        .map_err(|e| format!("PTY flush failed: {e}"))?;

    // Write 2: Enter (separate write for Ink agent compat)
    session
        .writer
        .write_all(b"\r")
        .map_err(|e| format!("PTY write \\r failed: {e}"))?;
    session
        .writer
        .flush()
        .map_err(|e| format!("PTY flush failed: {e}"))?;

    Ok(())
}

/// Write raw bytes to a PTY (for send_key).
fn raw_pty_write(state: &AppState, session_id: &str, data: &[u8]) -> Result<(), String> {
    let entry = state
        .sessions
        .get(session_id)
        .ok_or_else(|| format!("Session not found: {session_id}"))?;
    let mut session = entry.lock();
    session
        .writer
        .write_all(data)
        .map_err(|e| format!("PTY write failed: {e}"))?;
    session
        .writer
        .flush()
        .map_err(|e| format!("PTY flush failed: {e}"))?;
    Ok(())
}

// ── Tool execution ────────────────────────────────────────────

/// Execute `read_screen`: return visible terminal text.
///
/// With `since_cursor`: returns JSON `{"screen": "...", "cursor": N}` with only new
/// log lines since the given cursor position (from a previous call).
/// Without `since_cursor`: returns JSON `{"screen": "...", "cursor": N}` with the
/// current viewport snapshot.
fn exec_read_screen(state: &AppState, args: &Value) -> ToolResult {
    let session_id = match args["session_id"].as_str() {
        Some(s) => s,
        None => return ToolResult::err("Missing session_id"),
    };
    // Cap at 500 (matches drive_agent) so a model can't pull the whole ring
    // buffer in delta mode; the non-delta path is already viewport-bounded.
    let max_lines = (args["lines"].as_u64().unwrap_or(50) as usize).min(500);
    let since_cursor = args["since_cursor"].as_u64().map(|v| v as usize);

    let vt_log = match state.vt_log_buffers.get(session_id) {
        Some(v) => v,
        None => return ToolResult::err(format!("No VT buffer for session: {session_id}")),
    };
    let vt = vt_log.lock();

    let (text, cursor) = if let Some(since) = since_cursor {
        let (log_lines, new_cursor) = vt.lines_since_owned(since, max_lines);
        let content: Vec<String> = log_lines.iter().map(|ll| ll.text()).collect();
        (content.join("\n"), new_cursor)
    } else {
        let cursor = vt.total_lines();
        let rows = vt.screen_rows();
        // Trim trailing empty rows and limit
        let last_non_empty = rows
            .iter()
            .rposition(|r| !r.trim().is_empty())
            .map(|i| i + 1)
            .unwrap_or(0);
        let visible = &rows[..last_non_empty.min(max_lines)];
        (visible.join("\n"), cursor)
    };

    // Attach live session state so the model can tell whether the agent is
    // working (busy/spinner) vs paused — the screen text alone (e.g. a static
    // "Caramelizing…" spinner frame) is not enough to infer activity.
    let mut payload = json!({"screen": redact_secrets(&text), "cursor": cursor});
    if let Some(ss) = state.session_state_with_shell(session_id)
        && let Some(obj) = payload.as_object_mut()
    {
        if let Some(shell_state) = ss.shell_state {
            obj.insert("shell_state".into(), json!(shell_state));
        }
        obj.insert("awaiting_input".into(), json!(ss.awaiting_input));
        if let Some(intent) = ss.agent_intent {
            obj.insert("agent_intent".into(), json!(intent));
        }
        if let Some(agent_type) = ss.agent_type {
            obj.insert("agent_type".into(), json!(agent_type));
        }
    }
    ToolResult::ok(payload.to_string())
}

fn exec_search_scrollback(state: &AppState, args: &Value) -> ToolResult {
    let session_id = match args["session_id"].as_str() {
        Some(s) => s,
        None => return ToolResult::err("Missing session_id"),
    };
    let query = match args["query"].as_str() {
        Some(s) if !s.is_empty() => s,
        _ => return ToolResult::err("Missing query"),
    };
    let limit = (args["limit"].as_u64().unwrap_or(50) as usize).min(500);

    let vt_log = match state.vt_log_buffers.get(session_id) {
        Some(v) => v,
        None => return ToolResult::err(format!("No VT buffer for session: {session_id}")),
    };
    let matches: Vec<Value> = vt_log
        .lock()
        .grid_search_buffer(query)
        .into_iter()
        .take(limit)
        .map(|m| {
            json!({
                "line_index": m.line_index,
                "line_text": redact_secrets(&m.line_text),
                "match_start": m.match_start,
                "match_end": m.match_end,
            })
        })
        .collect();
    ToolResult::ok(json!({ "matches": matches }).to_string())
}

fn exec_get_hyperlinks(state: &AppState, args: &Value) -> ToolResult {
    let session_id = match args["session_id"].as_str() {
        Some(s) => s,
        None => return ToolResult::err("Missing session_id"),
    };
    let vt_log = match state.vt_log_buffers.get(session_id) {
        Some(v) => v,
        None => return ToolResult::err(format!("No VT buffer for session: {session_id}")),
    };
    let links: Vec<Value> = vt_log
        .lock()
        .grid_enumerate_hyperlinks()
        .into_iter()
        .map(|(row, start, end, uri)| {
            json!({
                "line_index": row,
                "start_col": start,
                "end_col": end,
                "uri": redact_secrets(&uri),
            })
        })
        .collect();
    ToolResult::ok(json!({ "hyperlinks": links }).to_string())
}

fn exec_get_semantic_zones(state: &AppState, args: &Value) -> ToolResult {
    let session_id = match args["session_id"].as_str() {
        Some(s) => s,
        None => return ToolResult::err("Missing session_id"),
    };
    let vt_log = match state.vt_log_buffers.get(session_id) {
        Some(v) => v,
        None => return ToolResult::err(format!("No VT buffer for session: {session_id}")),
    };
    let zones: Vec<Value> = vt_log
        .lock()
        .grid_extract_semantic_zones()
        .into_iter()
        .map(|(kind, start, end, text)| {
            json!({
                "kind": kind,
                "start_line": start,
                "end_line": end,
                "text": redact_secrets(&text),
            })
        })
        .collect();
    ToolResult::ok(json!({ "zones": zones }).to_string())
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
        let checker = RegexSafetyChecker::get();
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

    if !skip_safety
        && let Some(sk) = safe_key
        && sk.risk() == KeyRisk::High
    {
        return ToolResult::approval(
            format!("{key_name} is high-risk (may terminate shell)"),
            format!("send_key:{key_name}"),
        );
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

    const MAX_PATTERN_BYTES: usize = 512;
    let compiled = match pattern {
        Some(p) if p.len() > MAX_PATTERN_BYTES => {
            return ToolResult::err(format!("Pattern too long (max {MAX_PATTERN_BYTES} bytes)"));
        }
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
            if compiled.is_some() {
                return ToolResult::err("Timeout: pattern not matched within stability window");
            }
            return ToolResult::err("Timeout waiting for stability");
        }

        let current = {
            let vt_log = match state.vt_log_buffers.get(&session_id) {
                Some(v) => v,
                None => return ToolResult::err(format!("No VT buffer for session: {session_id}")),
            };
            let vt = vt_log.lock();
            vt.screen_rows().join("\n")
        };

        if let Some(ref re) = compiled
            && let Some(m) = re.find(&current)
        {
            return ToolResult::ok(redact_secrets(m.as_str()));
        }

        if current != last_content {
            last_content = current;
            stable_since = tokio::time::Instant::now();
        } else if stable_since.elapsed() >= std::time::Duration::from_millis(stability_ms) {
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

    let Some(entry) = state.session_states.get(session_id) else {
        return ToolResult::err(format!("No state for session: {session_id}"));
    };
    match serde_json::to_value(entry.value()) {
        Ok(v) => ToolResult::ok(v.to_string()),
        Err(e) => {
            tracing::warn!(session_id, error = %e, "Failed to serialize session state");
            ToolResult::err(format!("Failed to serialize state: {e}"))
        }
    }
}

/// Execute `get_context`: compact context string (~500 chars).
fn exec_get_context(state: &AppState, args: &Value) -> ToolResult {
    let session_id = match args["session_id"].as_str() {
        Some(s) => s,
        None => return ToolResult::err("Missing session_id"),
    };

    let shell_state = state
        .shell_states
        .get(session_id)
        .map(|atom| {
            crate::pty::shell_state_str(atom.load(std::sync::atomic::Ordering::Relaxed)).to_string()
        })
        .unwrap_or_else(|| "unknown".to_string());

    let ss = state.session_states.get(session_id);
    let agent_type = ss
        .as_ref()
        .and_then(|s| s.agent_type.clone())
        .unwrap_or_else(|| "none".to_string());
    let terminal_mode = ss
        .as_ref()
        .and_then(|s| s.terminal_mode.as_ref())
        .map(|m| serde_json::to_string(m).unwrap_or_default())
        .unwrap_or_else(|| "shell".to_string());
    drop(ss);

    // Working directory from the live PtySession.
    let cwd = state
        .sessions
        .get(session_id)
        .and_then(|s| s.lock().cwd.clone());

    // Git branch via a cheap .git/HEAD read — no subprocess, no index lock.
    let git_branch = cwd
        .as_deref()
        .and_then(|c| crate::git::read_branch_from_head(std::path::Path::new(c)));

    // Most recent command's exit code from the OSC 133 knowledge store.
    let last_exit_code = state
        .session_knowledge
        .get(session_id)
        .and_then(|entry| entry.lock().commands.back().and_then(|c| c.exit_code));

    let context = json!({
        "shell_state": shell_state,
        "agent_type": agent_type,
        "terminal_mode": terminal_mode,
        "cwd": cwd,
        "git_branch": git_branch,
        "last_exit_code": last_exit_code,
        "session_id": session_id,
    });

    ToolResult::ok(context.to_string())
}

/// Maps a `CommandOutcome` classification to its `error_type`, if any.
fn outcome_error_type(class: &crate::ai_agent::knowledge::OutcomeClass) -> Option<&str> {
    match class {
        crate::ai_agent::knowledge::OutcomeClass::Error { error_type } => Some(error_type.as_str()),
        _ => None,
    }
}

/// Execute `get_command_history`: recent OSC 133 command outcomes, newest first.
fn exec_get_command_history(state: &AppState, args: &Value) -> ToolResult {
    let session_id = match args["session_id"].as_str() {
        Some(s) => s,
        None => return ToolResult::err("Missing session_id"),
    };
    let limit = (args["limit"].as_u64().unwrap_or(20) as usize).min(200);
    let errors_only = args["errors_only"].as_bool().unwrap_or(false);

    let Some(entry) = state.session_knowledge.get(session_id) else {
        return ToolResult::ok(json!({"commands": []}).to_string());
    };
    let k = entry.lock();
    let commands: Vec<Value> = k
        .commands
        .iter()
        .rev()
        .filter(|c| !errors_only || outcome_error_type(&c.classification).is_some())
        .take(limit)
        .map(|c| {
            json!({
                "command": c.command,
                "cwd": c.cwd,
                "exit_code": c.exit_code,
                "duration_ms": c.duration_ms,
                "error_type": outcome_error_type(&c.classification),
                "timestamp": c.timestamp,
            })
        })
        .collect();
    ToolResult::ok(json!({"commands": commands}).to_string())
}

/// Execute `explain_last_failure`: the most recent failed command + its output.
fn exec_explain_last_failure(state: &AppState, args: &Value) -> ToolResult {
    let session_id = match args["session_id"].as_str() {
        Some(s) => s,
        None => return ToolResult::err("Missing session_id"),
    };
    let Some(entry) = state.session_knowledge.get(session_id) else {
        return ToolResult::ok(json!({"found": false}).to_string());
    };
    let k = entry.lock();
    let failure = k.commands.iter().rev().find(|c| {
        outcome_error_type(&c.classification).is_some() || c.exit_code.is_some_and(|e| e != 0)
    });
    match failure {
        Some(c) => ToolResult::ok(
            json!({
                "found": true,
                "command": c.command,
                "exit_code": c.exit_code,
                "error_type": outcome_error_type(&c.classification),
                "output": c.output_snippet,
                "cwd": c.cwd,
                "duration_ms": c.duration_ms,
            })
            .to_string(),
        ),
        None => ToolResult::ok(json!({"found": false}).to_string()),
    }
}

/// Execute `get_error_fixes`: known error→fix correlations for this session.
fn exec_get_error_fixes(state: &AppState, args: &Value) -> ToolResult {
    let session_id = match args["session_id"].as_str() {
        Some(s) => s,
        None => return ToolResult::err("Missing session_id"),
    };
    let Some(entry) = state.session_knowledge.get(session_id) else {
        return ToolResult::ok(json!({"fixes": []}).to_string());
    };
    let k = entry.lock();
    let fixes: Vec<Value> = k
        .error_fix_pairs
        .iter()
        .map(|(err, cmds)| json!({"error_type": err, "fix_commands": cmds}))
        .collect();
    ToolResult::ok(json!({"fixes": fixes}).to_string())
}

// ── Drive agent (atomic send→wait→read) ─────────────────────

async fn exec_drive_agent(state: &Arc<AppState>, args: &Value, skip_safety: bool) -> ToolResult {
    let session_id = match args["session_id"].as_str() {
        Some(s) => s.to_string(),
        None => return ToolResult::err("Missing session_id"),
    };
    let command = args["command"].as_str();
    let timeout_ms = args["timeout_ms"].as_u64().unwrap_or(30_000).min(120_000);
    let wait_pattern = args["wait_pattern"].as_str();
    let lines = args["lines"].as_u64().unwrap_or(80).min(500) as usize;

    const MAX_PATTERN_BYTES: usize = 512;
    let compiled = match wait_pattern {
        Some(p) if p.len() > MAX_PATTERN_BYTES => {
            return ToolResult::err(format!(
                "wait_pattern too long (max {MAX_PATTERN_BYTES} bytes)"
            ));
        }
        Some(p) => match regex::Regex::new(p) {
            Ok(r) => Some(r),
            Err(e) => return ToolResult::err(format!("Invalid wait_pattern regex: {e}")),
        },
        None => None,
    };

    // Step 1: send command (if provided)
    if let Some(cmd) = command {
        if !skip_safety {
            let checker = RegexSafetyChecker::get();
            match checker.evaluate(cmd) {
                SafetyVerdict::Allow => {}
                SafetyVerdict::NeedsApproval { reason } => {
                    return ToolResult::approval(reason, cmd);
                }
                verdict => {
                    let rejection = super::safety::format_rejection(&verdict).unwrap_or_default();
                    return ToolResult::err(rejection);
                }
            }
        }
        if let Err(e) = safe_pty_write(state, &session_id, cmd) {
            return ToolResult::err(e);
        }
        // Brief pause to let the PTY register the input before polling
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // Step 2: wait for idle or pattern
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    let stability_ms = 1500u64;
    let mut last_content = String::new();
    let mut stable_since = tokio::time::Instant::now();
    let mut pattern_matched = false;

    loop {
        if tokio::time::Instant::now() >= deadline {
            break; // timeout — still return what we have
        }

        // Check shell_state for idle (most reliable signal)
        let is_idle = state
            .shell_states
            .get(&session_id)
            .map(|atom| {
                let s = atom.load(std::sync::atomic::Ordering::Relaxed);
                crate::pty::shell_state_str(s) == "idle"
            })
            .unwrap_or(false);

        let current = {
            let vt_log = match state.vt_log_buffers.get(&session_id) {
                Some(v) => v,
                None => return ToolResult::err(format!("No VT buffer for session: {session_id}")),
            };
            vt_log.lock().screen_rows().join("\n")
        };

        // Pattern match takes priority
        if let Some(ref re) = compiled
            && re.is_match(&current)
        {
            pattern_matched = true;
            break;
        }

        // Shell idle after command was sent = done
        if command.is_some() && is_idle {
            break;
        }

        // Stability fallback (no command, or shell_state not available)
        if current != last_content {
            last_content = current;
            stable_since = tokio::time::Instant::now();
        } else if stable_since.elapsed() >= std::time::Duration::from_millis(stability_ms) {
            break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // Step 3: read screen + state
    let since_cursor = args["since_cursor"].as_u64().map(|v| v as usize);
    let vt_log = match state.vt_log_buffers.get(&session_id) {
        Some(v) => v,
        None => return ToolResult::err(format!("No VT buffer for session: {session_id}")),
    };
    let (screen_text, cursor) = {
        let buf = vt_log.lock();
        if let Some(since) = since_cursor {
            let (log_lines, new_cursor) = buf.lines_since_owned(since, lines);
            let text: Vec<String> = log_lines.iter().map(|ll| ll.text()).collect();
            (text.join("\n"), new_cursor)
        } else {
            let cursor = buf.total_lines();
            let rows = buf.screen_rows();
            let take = rows.len().min(lines);
            (rows[rows.len() - take..].join("\n"), cursor)
        }
    };

    let session_state = state
        .session_states
        .get(&session_id)
        .and_then(|entry| serde_json::to_value(entry.value()).ok());

    let shell_state = state
        .shell_states
        .get(&session_id)
        .map(|atom| {
            crate::pty::shell_state_str(atom.load(std::sync::atomic::Ordering::Relaxed)).to_string()
        })
        .unwrap_or_else(|| "unknown".to_string());

    let result = json!({
        "screen": redact_secrets(&screen_text),
        "cursor": cursor,
        "shell_state": shell_state,
        "session_state": session_state,
        "pattern_matched": pattern_matched,
    });

    ToolResult::ok(result.to_string())
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

fn is_session_unrestricted(state: &AppState, session_id: &str) -> bool {
    state.unrestricted_sessions.contains_key(session_id)
}

/// Resolve a file path for reading. Absolute paths are used as-is (unrestricted);
/// relative paths are always anchored to the sandbox root so `read_file("src/main.rs")`
/// works naturally. The sandbox jail is bypassed for reads — only writes are sandboxed.
fn resolve_file_path(
    sandbox: &FileSandbox,
    path: &str,
    _unrestricted: bool,
) -> Result<std::path::PathBuf, String> {
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        Ok(p.to_path_buf())
    } else {
        // Relative paths anchor to sandbox root, same as sandboxed behavior.
        sandbox.resolve(path)
    }
}

/// Extra write roots allowed in standard mode: `/tmp` and its macOS canonical
/// form `/private/tmp` (since `/tmp` is a symlink on macOS).
fn extra_write_roots() -> Vec<std::path::PathBuf> {
    let mut roots = vec![std::path::PathBuf::from("/tmp")];
    if let Ok(canon) = std::path::Path::new("/tmp").canonicalize()
        && canon != std::path::Path::new("/tmp")
    {
        roots.push(canon);
    }
    roots
}

/// Resolve a file path for writing. In unrestricted mode parent dirs are created
/// as needed; in standard mode the path is validated against the sandbox jail
/// (writes to /tmp are also allowed in standard mode).
fn resolve_file_path_for_write(
    sandbox: &FileSandbox,
    path: &str,
    unrestricted: bool,
) -> Result<std::path::PathBuf, String> {
    if unrestricted {
        let p = std::path::PathBuf::from(path);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("create_dir_all failed: {e}"))?;
        }
        Ok(p)
    } else {
        sandbox.resolve_for_write_with_extra_roots(path, &extra_write_roots())
    }
}

/// Resolve an optional subdirectory for read ops. Absolute paths are allowed
/// anywhere; relative paths and empty/omitted paths default to the sandbox root.
fn resolve_subdir(
    sandbox: &FileSandbox,
    path: Option<&str>,
    _unrestricted: bool,
) -> Result<std::path::PathBuf, String> {
    match path {
        None | Some("") | Some(".") => Ok(sandbox.root().to_path_buf()),
        Some(p) => {
            let pb = std::path::Path::new(p);
            if pb.is_absolute() {
                Ok(pb.to_path_buf())
            } else {
                resolve_sandbox_subdir(sandbox, Some(p))
            }
        }
    }
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
    let requested_limit = args["limit"]
        .as_u64()
        .map(|v| v as usize)
        .unwrap_or(READ_FILE_DEFAULT_LINES);
    let limit = requested_limit.clamp(1, READ_FILE_MAX_LINES);

    let sandbox = match get_sandbox(state, session_id) {
        Ok(s) => s,
        Err(e) => return ToolResult::err(e),
    };
    // Reads are unrestricted — developers legitimately need /etc/hosts, ~/.gitconfig, etc.
    let resolved = match resolve_file_path(&sandbox, file_path, true) {
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

fn exec_write_file_inner(
    state: &AppState,
    session_id: &str,
    args: &Value,
    skip_safety: bool,
) -> ToolResult {
    let Some(file_path) = args["file_path"].as_str() else {
        return missing_arg("file_path");
    };
    let Some(content) = args["content"].as_str() else {
        return missing_arg("content");
    };

    if !skip_safety {
        let checker = RegexSafetyChecker::get();
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
    let unrestricted = is_session_unrestricted(state, session_id);
    let resolved = match resolve_file_path_for_write(&sandbox, file_path, unrestricted) {
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

fn exec_edit_file_inner(
    state: &AppState,
    session_id: &str,
    args: &Value,
    skip_safety: bool,
) -> ToolResult {
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
        let checker = RegexSafetyChecker::get();
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
    let unrestricted = is_session_unrestricted(state, session_id);
    let resolved = match resolve_file_path(&sandbox, file_path, unrestricted) {
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
fn resolve_sandbox_subdir(
    sandbox: &FileSandbox,
    path: Option<&str>,
) -> Result<std::path::PathBuf, String> {
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
    // Reads are unrestricted — listing outside the repo root is legitimate.
    let anchor = match resolve_subdir(&sandbox, subdir, true) {
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
    // Reads are unrestricted — searching outside the repo root is legitimate.
    let anchor = match resolve_subdir(&sandbox, subdir, true) {
        Ok(p) => p,
        Err(e) => return ToolResult::err(e),
    };

    let re = match regex::Regex::new(pattern) {
        Ok(r) => r,
        Err(e) => return ToolResult::err(format!("invalid regex: {e}")),
    };

    let mut walk_builder = ignore::WalkBuilder::new(&anchor);
    walk_builder
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true);
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
    let mut files_with_matches: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
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

// ── search_code ────────────────────────────────────────────────

/// Max results returned by `search_code`.
const SEARCH_CODE_MAX_RESULTS: usize = 20;

fn exec_search_code(state: &Arc<AppState>, session_id: &str, args: &Value) -> ToolResult {
    let Some(query) = args["query"].as_str().filter(|q| !q.trim().is_empty()) else {
        return missing_arg("query");
    };
    let limit = args["limit"]
        .as_u64()
        .map(|v| v as usize)
        .unwrap_or(10)
        .min(SEARCH_CODE_MAX_RESULTS);

    let sandbox = match get_sandbox(state, session_id) {
        Ok(s) => s,
        Err(e) => return ToolResult::err(e),
    };

    let repo_root = sandbox.root().to_string_lossy().to_string();
    let index_arc = crate::content_index::ensure_index(state, &repo_root);
    let _guard = state.indexer_throttle.begin_search();

    let results = {
        let idx = index_arc.read();
        if !idx.is_ready() {
            return ToolResult::ok(
                json!({ "results": [], "note": "Index building in background, retry in a moment" })
                    .to_string(),
            );
        }
        idx.search(query, limit)
    };

    let query_words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();

    let out: Vec<Value> = results
        .into_iter()
        .map(|ranked| {
            let abs = index_arc.read().absolute_path(&ranked.rel_path);
            let snippet = extract_bm25_snippet(&abs, &query_words);
            json!({
                "path": ranked.rel_path,
                "score": ranked.score,
                "snippet": snippet,
            })
        })
        .collect();

    ToolResult::ok(json!({ "results": out }).to_string())
}

/// Read `path` and return a 3-line window around the line with the most query-word hits.
fn extract_bm25_snippet(path: &std::path::Path, query_words: &[String]) -> String {
    let Ok(content) = std::fs::read_to_string(path) else {
        return String::new();
    };
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return String::new();
    }
    let best = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let lower = line.to_lowercase();
            let hits = query_words
                .iter()
                .filter(|w| lower.contains(w.as_str()))
                .count();
            (i, hits)
        })
        .max_by_key(|(_, hits)| *hits)
        .map(|(i, _)| i)
        .unwrap_or(0);
    let start = best.saturating_sub(1);
    let end = (best + 2).min(lines.len());
    lines[start..end].join("\n")
}

// ── search_tools ───────────────────────────────────────────────

fn exec_search_tools(state: &AppState, args: &Value) -> ToolResult {
    let query = args["query"].as_str().map(|s| s.to_lowercase());
    let limit = args["limit"].as_u64().unwrap_or(20).min(100) as usize;

    let all_tools = state.mcp_upstream_registry.aggregated_tools();
    let descriptors: Vec<Value> = all_tools
        .into_iter()
        .filter(|tool| {
            let Some(q) = &query else { return true };
            let name = tool["name"].as_str().unwrap_or("").to_lowercase();
            let desc = tool["description"].as_str().unwrap_or("").to_lowercase();
            name.contains(q.as_str()) || desc.contains(q.as_str())
        })
        .take(limit)
        .map(|tool| {
            json!({
                "name": tool["name"],
                "description": tool["description"],
            })
        })
        .collect();

    let count = descriptors.len();
    ToolResult::ok(json!({ "tools": descriptors, "count": count }).to_string())
}

// ── call_tool ──────────────────────────────────────────────────

async fn exec_call_tool(state: &AppState, args: &Value) -> ToolResult {
    let Some(tool_name) = args["tool_name"].as_str().filter(|s| !s.is_empty()) else {
        return missing_arg("tool_name");
    };
    let call_args = if args["args"].is_null() {
        json!({})
    } else {
        args["args"].clone()
    };

    match state
        .mcp_upstream_registry
        .proxy_tool_call(tool_name, call_args)
        .await
    {
        Ok(result) => {
            let raw = result.to_string();
            let (output, _truncated) = truncate_output(&raw);
            ToolResult::ok(output)
        }
        Err(e) => ToolResult::err(e),
    }
}

// ── list_sessions ──────────────────────────────────────────────

fn exec_list_sessions(state: &AppState) -> ToolResult {
    let mut sessions: Vec<Value> = Vec::new();
    for entry_ref in state.sessions.iter() {
        let sid = entry_ref.key().clone();
        let pty = entry_ref.value().lock();
        let ss = state.session_states.get(&sid);
        let alias = state.term_aliases.get(&sid).map(|e| e.value().clone());
        sessions.push(json!({
            "session_id": sid,
            "alias": alias,
            "name": pty.display_name.as_deref().unwrap_or(""),
            "cwd": pty.cwd.as_deref().unwrap_or(""),
            "shell_state": ss.as_ref().and_then(|s| s.shell_state.as_deref()),
            "agent_type": ss.as_ref().and_then(|s| s.agent_type.as_deref()),
        }));
    }
    ToolResult::ok(json!({ "sessions": sessions, "count": sessions.len() }).to_string())
}

// ── spawn_session ──────────────────────────────────────────────

async fn exec_spawn_session(state: &Arc<AppState>, session_id: &str, args: &Value) -> ToolResult {
    let cwd = args["cwd"].as_str().map(|s| s.to_string()).or_else(|| {
        state
            .file_sandboxes
            .get(session_id)
            .map(|s| s.root().to_string_lossy().to_string())
    });
    let name = args["name"].as_str().map(|s| s.to_string());

    match crate::pty::spawn_session_for_agent(state, cwd, name).await {
        Ok(new_sid) => ToolResult::ok(json!({ "session_id": new_sid }).to_string()),
        Err(e) => ToolResult::err(format!("Failed to spawn session: {e}")),
    }
}

// ── get_agent_status ───────────────────────────────────────────

fn exec_get_agent_status(args: &Value) -> ToolResult {
    let Some(target) = args["target_session_id"].as_str() else {
        return missing_arg("target_session_id");
    };
    match super::conversation_engine::ACTIVE_CONVERSATIONS.get(target) {
        Some(handle) => {
            let state = *handle.state.read();
            ToolResult::ok(json!({ "session_id": target, "state": state }).to_string())
        }
        None => ToolResult::ok(json!({ "session_id": target, "state": null }).to_string()),
    }
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

async fn exec_run_command_inner(
    state: &AppState,
    session_id: &str,
    args: &Value,
    skip_safety: bool,
) -> ToolResult {
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
        let checker = RegexSafetyChecker::get();
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
    let unrestricted = is_session_unrestricted(state, session_id);
    let cwd = match resolve_subdir(&sandbox, cwd_arg, unrestricted) {
        Ok(p) => p,
        Err(e) => return ToolResult::err(e),
    };
    if !cwd.is_dir() {
        return ToolResult::err(format!("cwd is not a directory: {}", cwd.display()));
    }

    let home = sandbox.root().to_string_lossy().into_owned();
    let cwd_display = cwd.display().to_string();
    let start = std::time::Instant::now();

    let mut cmd;
    #[cfg(unix)]
    {
        let lang = std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".into());
        cmd = tokio::process::Command::new("sh");
        cmd.arg("-c")
            .arg(command)
            .current_dir(&cwd)
            .env_clear()
            .env("PATH", "/usr/local/bin:/usr/bin:/bin")
            .env("HOME", &home)
            .env("TERM", "xterm-256color")
            .env("LANG", &lang)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .process_group(0);
    }
    #[cfg(windows)]
    {
        cmd = tokio::process::Command::new("cmd.exe");
        cmd.arg("/C")
            .arg(command)
            .current_dir(&cwd)
            .env("USERPROFILE", &home)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        crate::cli::apply_no_window(cmd.as_std_mut());
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return ToolResult::err(format!("spawn failed: {e}")),
    };

    // Take ownership of pipes, then spawn concurrent read tasks.
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let stdout_task = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let Some(mut pipe) = stdout_pipe else {
            return Vec::new();
        };
        let mut buf = Vec::new();
        let _ = pipe.read_to_end(&mut buf).await;
        buf
    });
    let stderr_task = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let Some(mut pipe) = stderr_pipe else {
            return Vec::new();
        };
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
                // SAFETY: pid is from tokio Child::id(), a valid Unix PID. killpg
                // sends SIGKILL to the process group. The i32 cast is safe because
                // Unix PIDs fit in i32.
                unsafe {
                    libc::killpg(pid as i32, libc::SIGKILL);
                }
            }
            #[cfg(not(unix))]
            let _ = child.kill().await;
            let _ = child.wait().await;
            let _ = stdout_task.await;
            let _ = stderr_task.await;

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

// ── schedule_task / list_schedules / cancel_schedule ─────────────────────

fn exec_schedule_task(args: &Value) -> ToolResult {
    use super::scheduler::{ScheduledJob, load_config, save_config};

    let goal = match args["goal"].as_str() {
        Some(g) if !g.trim().is_empty() => g.trim().to_string(),
        _ => return ToolResult::err("goal is required"),
    };
    if goal.len() > 500 {
        return ToolResult::err("goal must be 500 characters or fewer");
    }

    let interval_minutes = match args["interval_minutes"].as_u64() {
        Some(m) if m >= 5 => m,
        Some(_) => return ToolResult::err("interval_minutes must be at least 5"),
        None => return ToolResult::err("interval_minutes is required"),
    };

    let one_shot = args["one_shot"].as_bool().unwrap_or(false);

    let mut config = load_config();

    let active_count = config.jobs.iter().filter(|j| j.enabled).count();
    if active_count >= 10 {
        return ToolResult::err(
            "Maximum 10 active scheduled jobs reached — cancel an existing job first",
        );
    }

    let id = format!(
        "agent-{}",
        uuid::Uuid::new_v4()
            .to_string()
            .split('-')
            .next()
            .unwrap_or("x")
    );
    // 6-field cron: sec min hour dom month dow
    let cron_expr = format!("0 0/{} * * * *", interval_minutes);

    let job = ScheduledJob {
        id: id.clone(),
        cron_expr,
        goal,
        target_session: None,
        max_duration_secs: super::scheduler::DEFAULT_MAX_DURATION_SECS,
        enabled: true,
        one_shot,
    };

    config.jobs.push(job);
    if let Err(e) = save_config(&config) {
        return ToolResult::err(format!("Failed to save schedule: {e}"));
    }

    ToolResult::ok(
        json!({ "id": id, "interval_minutes": interval_minutes, "one_shot": one_shot }).to_string(),
    )
}

fn exec_list_schedules() -> ToolResult {
    let config = super::scheduler::load_config();
    let jobs: Vec<Value> = config
        .jobs
        .iter()
        .map(|j| {
            json!({
                "id": j.id,
                "goal": j.goal,
                "cron_expr": j.cron_expr,
                "enabled": j.enabled,
                "one_shot": j.one_shot,
                "max_duration_secs": j.max_duration_secs,
            })
        })
        .collect();
    ToolResult::ok(serde_json::to_string(&jobs).unwrap_or_default())
}

fn exec_cancel_schedule(args: &Value) -> ToolResult {
    let id = match args["id"].as_str() {
        Some(id) if !id.trim().is_empty() => id.trim().to_string(),
        _ => return ToolResult::err("id is required"),
    };

    let mut config = super::scheduler::load_config();
    let before = config.jobs.len();
    config.jobs.retain(|j| j.id != id);
    if config.jobs.len() == before {
        return ToolResult::err(format!("No job found with id '{id}'"));
    }

    if let Err(e) = super::scheduler::save_config(&config) {
        return ToolResult::err(format!("Failed to save schedule: {e}"));
    }

    ToolResult::ok(json!({ "cancelled": id }).to_string())
}

// ── watch_for / list_watches / cancel_watch ──────────────────────────────
// Reactive watches reuse the WatcherEngine (cooldown/burst/max_fires safety).
// All watches are scoped to the agent's bound session_id — never args — so an
// agent cannot arm or cancel watches on another session.

fn exec_watch_for(
    state: &AppState,
    session_id: &str,
    args: &Value,
    skip_safety: bool,
) -> ToolResult {
    let trigger: watcher::WatcherTrigger = match serde_json::from_value(args["trigger"].clone()) {
        Ok(t) => t,
        Err(e) => return ToolResult::err(format!("Invalid trigger: {e}")),
    };
    let instructions = match args["instructions"].as_str() {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => return ToolResult::err("Missing instructions"),
    };

    if !skip_safety {
        let td = serde_json::to_string(&trigger).unwrap_or_default();
        let preview: String = instructions.chars().take(80).collect();
        return ToolResult::approval(
            format!("Arm reactive watch ({td}) → {preview}"),
            format!("watch_for {td}"),
        );
    }

    let Some(engine) = state.watcher_engine.get() else {
        return ToolResult::err("Watcher engine not initialized");
    };

    let rule = watcher::WatcherRule {
        id: String::new(),
        name: args["name"].as_str().unwrap_or("model watch").to_string(),
        session_id: Some(session_id.to_string()),
        template_id: None,
        prompt_id: None,
        repo_path: None,
        trigger,
        instructions: Some(instructions),
        max_fires: args["max_fires"]
            .as_u64()
            .map(|v| v as u32)
            .unwrap_or(3)
            .max(1),
        fire_count: 0,
        cooldown_secs: args["cooldown_secs"]
            .as_u64()
            .map(|v| (v as u32).max(5))
            .unwrap_or_else(watcher::default_cooldown),
        burst_threshold: watcher::default_burst_threshold(),
        burst_window_secs: watcher::default_burst_window(),
        status: watcher::WatcherStatus::Active,
        created_at: 0,
    };

    let cfg = engine.config();
    let mut config = cfg.write();
    match watcher::create_rule(&mut config, rule) {
        Ok(id) => ToolResult::ok(json!({ "watch_id": id, "status": "armed" }).to_string()),
        Err(e) => ToolResult::err(format!("Failed to arm watch: {e}")),
    }
}

fn exec_list_watches(state: &AppState, session_id: &str) -> ToolResult {
    let Some(engine) = state.watcher_engine.get() else {
        return ToolResult::err("Watcher engine not initialized");
    };
    let cfg = engine.config();
    let config = cfg.read();
    let watches: Vec<Value> = config
        .rules
        .iter()
        .filter(|r| r.session_id.as_deref() == Some(session_id))
        .map(|r| {
            json!({
                "watch_id": r.id,
                "name": r.name,
                "trigger": r.trigger,
                "status": r.status,
                "fire_count": r.fire_count,
                "max_fires": r.max_fires,
            })
        })
        .collect();
    ToolResult::ok(json!({ "watches": watches }).to_string())
}

fn exec_cancel_watch(state: &AppState, session_id: &str, args: &Value) -> ToolResult {
    let watch_id = match args["watch_id"].as_str() {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return ToolResult::err("Missing watch_id"),
    };
    let Some(engine) = state.watcher_engine.get() else {
        return ToolResult::err("Watcher engine not initialized");
    };
    let cfg = engine.config();
    let mut config = cfg.write();
    // Scope to the agent's session: only cancel a watch this session owns.
    let owned = config
        .rules
        .iter()
        .any(|r| r.id == watch_id && r.session_id.as_deref() == Some(session_id));
    if !owned {
        return ToolResult::err(format!("No watch '{watch_id}' on this session"));
    }
    match watcher::delete_rule(&mut config, &watch_id) {
        Ok(()) => ToolResult::ok(json!({ "cancelled": watch_id }).to_string()),
        Err(e) => ToolResult::err(format!("Failed to cancel watch: {e}")),
    }
}

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
    // Resolve alias → UUID in session_id arg (e.g. "tc-1" → actual UUID)
    let args = if let Some(sid) = args["session_id"].as_str() {
        if let Some(resolved) = state.resolve_alias(sid) {
            let mut a = args.clone();
            a["session_id"] = serde_json::Value::String(resolved);
            a
        } else {
            args.clone()
        }
    } else {
        args.clone()
    };
    let args = &args;

    // Cross-session reads (read_screen, drive_agent without command) are permitted.
    // Only writes (send_input, send_key, drive_agent with command) are restricted.
    let is_cross_session_write = matches!(fn_name, "send_input" | "send_key")
        || (fn_name == "drive_agent" && args["command"].is_string());
    if is_cross_session_write
        && let Some(target) = args["session_id"].as_str()
        && target != session_id
        && !is_session_unrestricted(state, session_id)
    {
        return ToolResult::err(format!(
            "Permission denied: agent bound to session {session_id} cannot write to {target}. Enable unrestricted mode for cross-session control."
        ));
    }
    match fn_name {
        "read_screen" => exec_read_screen(state, args),
        "search_scrollback" => exec_search_scrollback(state, args),
        "get_hyperlinks" => exec_get_hyperlinks(state, args),
        "get_semantic_zones" => exec_get_semantic_zones(state, args),
        "send_input" => exec_send_input_inner(state, args, skip_safety),
        "send_key" => exec_send_key_inner(state, args, skip_safety),
        "wait_for" => exec_wait_for(state, args).await,
        "get_state" => exec_get_state(state, args),
        "get_context" => exec_get_context(state, args),
        "get_command_history" => exec_get_command_history(state, args),
        "explain_last_failure" => exec_explain_last_failure(state, args),
        "get_error_fixes" => exec_get_error_fixes(state, args),
        "read_file" => exec_read_file(state, session_id, args),
        "write_file" => exec_write_file_inner(state, session_id, args, skip_safety),
        "edit_file" => exec_edit_file_inner(state, session_id, args, skip_safety),
        "list_files" => exec_list_files(state, session_id, args),
        "search_files" => exec_search_files(state, session_id, args),
        "search_code" => exec_search_code(state, session_id, args),
        "run_command" => exec_run_command_inner(state, session_id, args, skip_safety).await,
        "search_tools" => exec_search_tools(state, args),
        "call_tool" => exec_call_tool(state, args).await,
        "list_sessions" => exec_list_sessions(state),
        "spawn_session" => exec_spawn_session(state, session_id, args).await,
        "get_agent_status" => exec_get_agent_status(args),
        "drive_agent" => exec_drive_agent(state, args, skip_safety).await,
        "schedule_task" => exec_schedule_task(args),
        "list_schedules" => exec_list_schedules(),
        "cancel_schedule" => exec_cancel_schedule(args),
        "watch_for" => exec_watch_for(state, session_id, args, skip_safety),
        "list_watches" => exec_list_watches(state, session_id),
        "cancel_watch" => exec_cancel_watch(state, session_id, args),
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
    fn definitions_returns_31_tools() {
        let defs = tool_definitions();
        let arr = defs.as_array().unwrap();
        assert_eq!(arr.len(), 31);
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
                "get_command_history",
                "explain_last_failure",
                "get_error_fixes",
                "read_file",
                "write_file",
                "edit_file",
                "list_files",
                "search_files",
                "search_code",
                "run_command",
                "search_tools",
                "call_tool",
                "list_sessions",
                "spawn_session",
                "get_agent_status",
                "drive_agent",
                "schedule_task",
                "list_schedules",
                "cancel_schedule",
                "watch_for",
                "list_watches",
                "cancel_watch",
                "search_scrollback",
                "get_hyperlinks",
                "get_semantic_zones",
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
    fn redact_github_pat() {
        let pat = format!("github_pat_{}", "A".repeat(82));
        let output = redact_secrets(&format!("token: {pat}"));
        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("github_pat_"));
    }

    #[test]
    fn redact_pem_body() {
        let input = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA\nbase64data\n-----END RSA PRIVATE KEY-----";
        let output = redact_secrets(input);
        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("MIIEpAIBAAKCAQEA"));
    }

    /// Hex preceded by a secret-context word still gets redacted.
    #[test]
    fn redact_hex_with_secret_context() {
        let hex = "a".repeat(40);
        let output = redact_secrets(&format!("token={hex}"));
        assert!(output.contains("[REDACTED]"), "got: {output}");
        assert!(
            output.starts_with("token="),
            "context word must be preserved: {output}"
        );

        let output = redact_secrets(&format!("api_key: {hex}"));
        assert!(output.contains("[REDACTED]"), "got: {output}");
    }

    /// Regression for #1369-f051: bare 40-hex strings (git SHAs, lockfile hashes)
    /// must NOT be redacted. The old `\b[0-9a-fA-F]{40,}\b` catch-all mangled
    /// `git log/show/diff` and Cargo.lock / package-lock.json output.
    #[test]
    fn preserves_git_sha_and_lockfile_hashes() {
        // git log line — SHA-1 (40 hex)
        let git_log = "commit 1a3b5c7d9e0f1234567890abcdef1234567890ab\nAuthor: Boss";
        assert_eq!(redact_secrets(git_log), git_log);

        // git show / diff — full SHA in "index" line
        let diff = "index abcdef1234567890abcdef1234567890abcdef12..fedcba0987654321fedcba0987654321fedcba09 100644";
        assert_eq!(redact_secrets(diff), diff);

        // Cargo.lock — SHA-256 checksum (64 hex)
        let cargo_lock =
            r#"checksum = "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef""#;
        assert_eq!(redact_secrets(cargo_lock), cargo_lock);

        // package-lock.json — SHA-512 integrity hash (128 hex)
        let pnpm_hash = "b".repeat(128);
        let pkg_lock = format!(r#""integrity": "sha512-{pnpm_hash}=""#);
        assert_eq!(redact_secrets(&pkg_lock), pkg_lock);
    }

    #[test]
    fn no_redaction_on_safe_text() {
        let input = "$ cargo test\nrunning 32 tests\ntest result: ok";
        assert_eq!(redact_secrets(input), input);
    }

    #[test]
    fn redact_empty_string() {
        assert_eq!(redact_secrets(""), "");
    }

    #[test]
    fn redact_unicode_surrounding_secret() {
        let input = "日本語 sk-abc123def456ghi789jkl012mno345 中文";
        let output = redact_secrets(input);
        assert!(output.contains("[REDACTED]"));
        assert!(output.contains("日本語"));
        assert!(output.contains("中文"));
    }

    #[test]
    fn redact_large_input_no_panic() {
        let safe = "x".repeat(100_000);
        assert_eq!(redact_secrets(&safe), safe);
    }

    #[test]
    fn redact_multiple_secrets_same_line() {
        let input =
            "KEY1=sk-aaabbbccc111222333444555 KEY2=ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmn";
        let output = redact_secrets(input);
        assert!(!output.contains("sk-aaa"));
        assert!(!output.contains("ghp_"));
    }

    // ── .env key=value redaction ──────────────────────────────

    #[test]
    fn redact_stripe_secret_key() {
        let input = "STRIPE_SECRET_KEY=rk_live_abc123def456ghi789";
        let output = redact_secrets(input);
        assert!(
            !output.contains("rk_live_"),
            "secret value leaked: {output}"
        );
        assert!(
            output.contains("STRIPE_SECRET_KEY="),
            "key name lost: {output}"
        );
    }

    #[test]
    fn redact_db_password() {
        let input = "DB_PASSWORD=hunter2";
        let output = redact_secrets(input);
        assert!(!output.contains("hunter2"), "secret value leaked: {output}");
        assert!(output.contains("DB_PASSWORD="), "key name lost: {output}");
    }

    #[test]
    fn redact_my_secret_token() {
        let input = "MY_SECRET_TOKEN=abc123def456ghi789";
        let output = redact_secrets(input);
        assert!(
            !output.contains("abc123def456"),
            "secret value leaked: {output}"
        );
        assert!(
            output.contains("MY_SECRET_TOKEN="),
            "key name lost: {output}"
        );
    }

    #[test]
    fn no_redact_database_host() {
        let input = "DATABASE_HOST=localhost";
        assert_eq!(
            redact_secrets(input),
            input,
            "non-secret var was incorrectly redacted"
        );
    }

    #[test]
    fn no_redact_path_var() {
        let input = "PATH=/usr/bin:/usr/local/bin";
        assert_eq!(
            redact_secrets(input),
            input,
            "PATH was incorrectly redacted"
        );
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
        let result = dispatch(
            &state,
            "test",
            "read_screen",
            &json!({"session_id": "nope"}),
        )
        .await;
        assert!(!result.success);
        assert!(result.output.contains("No VT buffer"));
    }

    #[tokio::test]
    async fn read_screen_returns_cursor() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let vt = crate::state::VtLogBuffer::new(24, 80, 10_000);
        state
            .vt_log_buffers
            .insert("rs-test".to_string(), parking_lot::Mutex::new(vt));
        let result = dispatch(
            &state,
            "rs-test",
            "read_screen",
            &json!({
                "session_id": "rs-test"
            }),
        )
        .await;
        assert!(result.success, "should succeed: {}", result.output);
        let parsed: serde_json::Value = serde_json::from_str(&result.output).unwrap();
        assert!(
            parsed["cursor"].is_number(),
            "response must include cursor field"
        );
    }

    #[tokio::test]
    async fn read_screen_since_cursor_returns_delta() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut vt = crate::state::VtLogBuffer::new(24, 80, 10_000);
        for i in 0..30u32 {
            vt.process(format!("fill {i}\r\n").as_bytes());
        }
        let cursor_before = vt.total_lines();
        assert!(cursor_before > 0, "need scrollback");
        for i in 30..40u32 {
            vt.process(format!("new {i}\r\n").as_bytes());
        }
        state
            .vt_log_buffers
            .insert("rs-delta".to_string(), parking_lot::Mutex::new(vt));
        let result = dispatch(
            &state,
            "rs-delta",
            "read_screen",
            &json!({
                "session_id": "rs-delta",
                "since_cursor": cursor_before
            }),
        )
        .await;
        assert!(result.success, "should succeed: {}", result.output);
        let parsed: serde_json::Value = serde_json::from_str(&result.output).unwrap();
        assert!(parsed["cursor"].is_number(), "response must include cursor");
        assert!(parsed["screen"].is_string(), "response must include screen");
        let new_cursor = parsed["cursor"].as_u64().unwrap();
        assert!(new_cursor > cursor_before as u64, "cursor must advance");
    }

    #[tokio::test]
    async fn read_screen_includes_live_session_state() {
        // read_screen must surface shell_state/awaiting_input/agent_intent so the
        // model can tell a working agent (busy spinner) from a paused one.
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let vt = crate::state::VtLogBuffer::new(24, 80, 10_000);
        state
            .vt_log_buffers
            .insert("rs-state".to_string(), parking_lot::Mutex::new(vt));
        state.shell_states.insert(
            "rs-state".to_string(),
            std::sync::atomic::AtomicU8::new(crate::pty::SHELL_BUSY),
        );
        let mut ss = crate::state::SessionState {
            awaiting_input: true,
            agent_intent: Some("running tests".to_string()),
            ..Default::default()
        };
        ss.agent_type = Some("claude-code".to_string());
        state.session_states.insert("rs-state".to_string(), ss);

        let result = dispatch(
            &state,
            "rs-state",
            "read_screen",
            &json!({"session_id": "rs-state"}),
        )
        .await;
        assert!(result.success, "should succeed: {}", result.output);
        let parsed: serde_json::Value = serde_json::from_str(&result.output).unwrap();
        assert_eq!(
            parsed["shell_state"], "busy",
            "must surface live shell_state"
        );
        assert_eq!(
            parsed["awaiting_input"], true,
            "must surface awaiting_input"
        );
        assert_eq!(parsed["agent_intent"], "running tests");
        assert_eq!(parsed["agent_type"], "claude-code");
    }

    #[tokio::test]
    async fn knowledge_query_tools_read_osc133_outcomes() {
        use crate::ai_agent::knowledge::{CommandOutcome, OutcomeClass, SessionKnowledge};
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut k = SessionKnowledge::new();
        k.record(CommandOutcome {
            timestamp: 1,
            command: "cargo build".into(),
            cwd: "/repo".into(),
            exit_code: Some(101),
            output_snippet: "error[E0432]: unresolved import".into(),
            classification: OutcomeClass::Error {
                error_type: "rust_compilation".into(),
            },
            duration_ms: 1200,
            id: 0,
        });
        k.record(CommandOutcome {
            timestamp: 2,
            command: "cargo add foo".into(),
            cwd: "/repo".into(),
            exit_code: Some(0),
            output_snippet: "ok".into(),
            classification: OutcomeClass::Success,
            duration_ms: 50,
            id: 0,
        });
        state
            .session_knowledge
            .insert("kh".into(), parking_lot::Mutex::new(k));

        // get_command_history: newest first, both commands.
        let r = dispatch(
            &state,
            "kh",
            "get_command_history",
            &json!({"session_id": "kh"}),
        )
        .await;
        assert!(r.success, "{}", r.output);
        let v: serde_json::Value = serde_json::from_str(&r.output).unwrap();
        let cmds = v["commands"].as_array().unwrap();
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0]["command"], "cargo add foo");
        assert_eq!(cmds[1]["exit_code"], 101);

        // errors_only filter.
        let r = dispatch(
            &state,
            "kh",
            "get_command_history",
            &json!({"session_id": "kh", "errors_only": true}),
        )
        .await;
        let v: serde_json::Value = serde_json::from_str(&r.output).unwrap();
        let cmds = v["commands"].as_array().unwrap();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0]["error_type"], "rust_compilation");

        // explain_last_failure: finds the failing build.
        let r = dispatch(
            &state,
            "kh",
            "explain_last_failure",
            &json!({"session_id": "kh"}),
        )
        .await;
        let v: serde_json::Value = serde_json::from_str(&r.output).unwrap();
        assert_eq!(v["found"], true);
        assert_eq!(v["exit_code"], 101);
        assert_eq!(v["error_type"], "rust_compilation");
        assert!(v["output"].as_str().unwrap().contains("E0432"));

        // get_error_fixes: success-after-error correlated the fix.
        let r = dispatch(
            &state,
            "kh",
            "get_error_fixes",
            &json!({"session_id": "kh"}),
        )
        .await;
        let v: serde_json::Value = serde_json::from_str(&r.output).unwrap();
        let fixes = v["fixes"].as_array().unwrap();
        assert_eq!(fixes.len(), 1);
        assert_eq!(fixes[0]["error_type"], "rust_compilation");
        assert_eq!(fixes[0]["fix_commands"][0], "cargo add foo");
    }

    #[tokio::test]
    async fn knowledge_query_tools_empty_session() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let r = dispatch(
            &state,
            "none",
            "get_command_history",
            &json!({"session_id": "none"}),
        )
        .await;
        assert!(r.success);
        assert!(r.output.contains("\"commands\":[]"));
        let r = dispatch(
            &state,
            "none",
            "explain_last_failure",
            &json!({"session_id": "none"}),
        )
        .await;
        assert!(r.success);
        assert!(r.output.contains("\"found\":false"));
    }

    #[tokio::test]
    async fn dispatch_send_input_sudo_needs_approval() {
        // sudo moved from Block to NeedsApproval in the local-trust-boundary model.
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let result = dispatch(
            &state,
            "test",
            "send_input",
            &json!({
                "session_id": "test",
                "command": "sudo rm -rf /"
            }),
        )
        .await;
        assert!(!result.success);
        assert!(
            result.needs_approval,
            "expected needs_approval: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn dispatch_send_input_needs_approval_for_rm_rf() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let result = dispatch(
            &state,
            "test",
            "send_input",
            &json!({
                "session_id": "test",
                "command": "rm -rf /tmp/build"
            }),
        )
        .await;
        assert!(!result.success);
        assert!(result.needs_approval);
        assert!(result.approval_reason.is_some());
        assert_eq!(
            result.approval_command.as_deref(),
            Some("rm -rf /tmp/build")
        );
    }

    #[tokio::test]
    async fn dispatch_send_key_needs_approval_for_ctrl_d() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let result = dispatch(
            &state,
            "test",
            "send_key",
            &json!({
                "session_id": "test",
                "key": "ctrl-d"
            }),
        )
        .await;
        assert!(!result.success);
        assert!(result.needs_approval);
        assert!(result.approval_reason.unwrap().contains("high-risk"));
    }

    #[tokio::test]
    async fn dispatch_approved_bypasses_safety() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        // Normal dispatch returns needs_approval
        let result = dispatch(
            &state,
            "test",
            "send_input",
            &json!({
                "session_id": "test",
                "command": "rm -rf /tmp/build"
            }),
        )
        .await;
        assert!(result.needs_approval);
        // Approved dispatch skips safety (will fail on missing session, not safety)
        let result = dispatch_approved(
            &state,
            "test",
            "send_input",
            &json!({
                "session_id": "test",
                "command": "rm -rf /tmp/build"
            }),
        )
        .await;
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
        let result = dispatch(
            &state,
            "test",
            "get_context",
            &json!({"session_id": "nope"}),
        )
        .await;
        assert!(result.success); // Returns defaults for missing session
        assert!(result.output.contains("shell_state"));
    }

    #[tokio::test]
    async fn dispatch_wait_for_invalid_regex() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let result = dispatch(
            &state,
            "test",
            "wait_for",
            &json!({
                "session_id": "test",
                "pattern": "[invalid"
            }),
        )
        .await;
        assert!(!result.success);
        assert!(result.output.contains("Invalid regex"));
    }

    // ── drive_agent ──────────────────────────────────────���─────

    #[tokio::test]
    async fn drive_agent_missing_session_id() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let result = dispatch(&state, "test", "drive_agent", &json!({})).await;
        assert!(!result.success);
        assert!(result.output.contains("Missing session_id"));
    }

    #[tokio::test]
    async fn drive_agent_no_vt_buffer() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        // Read-only drive_agent (no command) should be allowed cross-session
        // but fail with VT buffer error for nonexistent session
        let result = dispatch(
            &state,
            "test",
            "drive_agent",
            &json!({
                "session_id": "nonexistent",
                "timeout_ms": 200
            }),
        )
        .await;
        assert!(!result.success, "should fail: {}", result.output);
        assert!(
            result.output.contains("No VT buffer"),
            "expected VT buffer error: {}",
            result.output
        );
    }

    #[tokio::test]
    async fn drive_agent_invalid_wait_pattern() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let result = dispatch(
            &state,
            "test",
            "drive_agent",
            &json!({
                "session_id": "test",
                "wait_pattern": "[bad"
            }),
        )
        .await;
        assert!(!result.success);
        assert!(result.output.contains("Invalid wait_pattern regex"));
    }

    #[tokio::test]
    async fn drive_agent_safety_check_on_command() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let result = dispatch(
            &state,
            "test",
            "drive_agent",
            &json!({
                "session_id": "test",
                "command": "sudo rm -rf /"
            }),
        )
        .await;
        assert!(!result.success);
        assert!(
            result.needs_approval,
            "dangerous command should require approval"
        );
    }

    #[tokio::test]
    async fn drive_agent_cross_session_write_blocked_without_unrestricted() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let result = dispatch(
            &state,
            "session-A",
            "drive_agent",
            &json!({
                "session_id": "session-B",
                "command": "echo hi"
            }),
        )
        .await;
        assert!(!result.success);
        assert!(
            result.output.contains("Permission denied"),
            "write should be blocked: {}",
            result.output
        );
    }

    #[tokio::test]
    async fn drive_agent_cross_session_read_only_allowed() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        // No command = read-only, should NOT be blocked by cross-session guard
        // (will fail for other reasons like missing VT buffer, but not Permission denied)
        let result = dispatch(
            &state,
            "session-A",
            "drive_agent",
            &json!({
                "session_id": "session-B",
                "timeout_ms": 200
            }),
        )
        .await;
        assert!(!result.success);
        assert!(
            !result.output.contains("Permission denied"),
            "read-only should not be blocked: {}",
            result.output
        );
    }

    #[tokio::test]
    async fn drive_agent_read_only_returns_state() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        // Insert a VtLogBuffer so the wait loop can read it
        let vt = crate::state::VtLogBuffer::new(24, 80, 10_000);
        state
            .vt_log_buffers
            .insert("test-read".to_string(), parking_lot::Mutex::new(vt));
        // Set shell state to idle so it returns immediately
        state.shell_states.insert(
            "test-read".to_string(),
            std::sync::atomic::AtomicU8::new(0), // 0 = idle
        );
        let result = dispatch(
            &state,
            "test-read",
            "drive_agent",
            &json!({
                "session_id": "test-read",
                "timeout_ms": 500
            }),
        )
        .await;
        assert!(
            result.success,
            "read-only drive_agent should succeed: {}",
            result.output
        );
        let parsed: serde_json::Value = serde_json::from_str(&result.output).unwrap();
        assert!(parsed["shell_state"].is_string());
        assert!(parsed["screen"].is_string());
    }

    #[tokio::test]
    async fn drive_agent_response_includes_cursor() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let vt = crate::state::VtLogBuffer::new(24, 80, 10_000);
        state
            .vt_log_buffers
            .insert("cur-test".to_string(), parking_lot::Mutex::new(vt));
        state
            .shell_states
            .insert("cur-test".to_string(), std::sync::atomic::AtomicU8::new(0));
        let result = dispatch(
            &state,
            "cur-test",
            "drive_agent",
            &json!({
                "session_id": "cur-test",
                "timeout_ms": 200
            }),
        )
        .await;
        assert!(result.success, "should succeed: {}", result.output);
        let parsed: serde_json::Value = serde_json::from_str(&result.output).unwrap();
        assert!(
            parsed["cursor"].is_number(),
            "response must include cursor field"
        );
    }

    #[tokio::test]
    async fn drive_agent_since_cursor_returns_delta() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut vt = crate::state::VtLogBuffer::new(24, 80, 10_000);
        // Fill viewport (24 rows) + overflow to push lines into scrollback log
        for i in 0..30u32 {
            vt.process(format!("line {i}\r\n").as_bytes());
        }
        let cursor_before = vt.total_lines();
        assert!(
            cursor_before > 0,
            "scrollback must have lines after overflow"
        );
        // Feed more lines after recording cursor
        for i in 30..40u32 {
            vt.process(format!("new {i}\r\n").as_bytes());
        }
        state
            .vt_log_buffers
            .insert("delta-test".to_string(), parking_lot::Mutex::new(vt));
        state.shell_states.insert(
            "delta-test".to_string(),
            std::sync::atomic::AtomicU8::new(0),
        );
        let result = dispatch(
            &state,
            "delta-test",
            "drive_agent",
            &json!({
                "session_id": "delta-test",
                "since_cursor": cursor_before,
                "timeout_ms": 200
            }),
        )
        .await;
        assert!(result.success, "should succeed: {}", result.output);
        let parsed: serde_json::Value = serde_json::from_str(&result.output).unwrap();
        assert!(parsed["cursor"].is_number(), "response must include cursor");
        let new_cursor = parsed["cursor"].as_u64().unwrap();
        assert!(
            new_cursor > cursor_before as u64,
            "cursor must advance after new lines"
        );
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
        let r = dispatch(
            &state,
            "nosession",
            "read_file",
            &json!({ "file_path": "x.txt" }),
        )
        .await;
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
        let r = dispatch(
            &state,
            "s1",
            "read_file",
            &json!({ "file_path": "big.txt" }),
        )
        .await;
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
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "x x x"
        );
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
            assert!(
                !r.output.contains("Unknown tool"),
                "tool {name} did not route"
            );
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
        let paths: Vec<&str> = entries
            .iter()
            .map(|e| e["path"].as_str().unwrap())
            .collect();
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
        let r = dispatch(
            &state,
            "s1",
            "list_files",
            &json!({ "pattern": "[unterminated" }),
        )
        .await;
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
        let r = dispatch(
            &state,
            "s1",
            "search_files",
            &json!({ "pattern": "needle" }),
        )
        .await;
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
        let r = dispatch(
            &state,
            "s1",
            "search_files",
            &json!({ "pattern": "needle" }),
        )
        .await;
        assert!(r.success, "{}", r.output);
        let parsed: Value = serde_json::from_str(&r.output).unwrap();
        let files = parsed["files_with_matches"].as_array().unwrap();
        assert!(
            files
                .iter()
                .any(|f| f.as_str().unwrap().ends_with("visible.txt"))
        );
        assert!(
            !files
                .iter()
                .any(|f| f.as_str().unwrap().ends_with("ignored.txt"))
        );
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
        let r = dispatch(
            &state,
            "s1",
            "search_files",
            &json!({ "pattern": "needle" }),
        )
        .await;
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
        let r = dispatch(
            &state,
            "none",
            "run_command",
            &json!({ "command": "echo hi" }),
        )
        .await;
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
        let r = dispatch(
            &state,
            "s1",
            "run_command",
            &json!({ "command": "echo hello_world" }),
        )
        .await;
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
        let r = dispatch(
            &state,
            "s1",
            "run_command",
            &json!({ "command": "exit 42" }),
        )
        .await;
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
        // `printenv HOME` returns the home directory path.
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
        // bare `env` is now allowed (local-trust-boundary model); verify it succeeds.
        let r2 = dispatch(&state, "s1", "run_command", &json!({ "command": "env" })).await;
        assert!(r2.success, "bare env should be allowed: {}", r2.output);
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

    // ── Cross-session identity binding ────────────────────────

    #[tokio::test]
    async fn cross_session_send_input_rejected() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let args = serde_json::json!({
            "session_id": "session-B",
            "command": "echo hello"
        });
        let result = dispatch_inner(&state, "session-A", "send_input", &args, false).await;
        assert!(!result.success);
        assert!(result.output.contains("Permission denied"));
        assert!(result.output.contains("session-A"));
        assert!(result.output.contains("session-B"));
    }

    #[tokio::test]
    async fn cross_session_send_key_rejected() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let args = serde_json::json!({
            "session_id": "session-B",
            "key": "ctrl-c"
        });
        let result = dispatch_inner(&state, "session-A", "send_key", &args, false).await;
        assert!(!result.success);
        assert!(result.output.contains("Permission denied"));
    }

    #[tokio::test]
    async fn same_session_send_input_not_rejected() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let args = serde_json::json!({
            "session_id": "session-A",
            "command": "echo hello"
        });
        let result = dispatch_inner(&state, "session-A", "send_input", &args, false).await;
        // May fail for other reasons (no PTY) but NOT for permission denied
        assert!(!result.output.contains("Permission denied"));
    }

    // ── search_code ────────────────────────────────────────────

    #[tokio::test]
    async fn search_code_requires_sandbox() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let r = dispatch(
            &state,
            "none",
            "search_code",
            &json!({ "query": "authentication" }),
        )
        .await;
        assert!(!r.success);
        assert!(r.output.contains("No filesystem sandbox"));
    }

    #[tokio::test]
    async fn search_code_missing_query() {
        let (_d, state) = fs_test_state("s1");
        let r = dispatch(&state, "s1", "search_code", &json!({})).await;
        assert!(!r.success);
        assert!(r.output.contains("query"));
    }

    #[tokio::test]
    async fn search_code_returns_ranked_results_with_snippet() {
        let (dir, state) = fs_test_state("s1");
        let repo_root = dir.path().to_path_buf();
        std::fs::write(
            repo_root.join("auth.rs"),
            "// authentication module\npub fn authenticate(token: &str) -> bool {\n    !token.is_empty()\n}\n",
        )
        .unwrap();
        std::fs::write(repo_root.join("other.rs"), "fn unrelated() {}\n").unwrap();

        // Build index synchronously and insert into state.
        // Use canonicalized path — FileSandbox::new canonicalizes, so the lookup key must match.
        let canonical_root = repo_root.canonicalize().unwrap();
        let index = crate::content_index::ContentIndex::build(
            canonical_root.clone(),
            None,
            std::collections::HashMap::new(),
        );
        let index_arc = Arc::new(parking_lot::RwLock::new(index));
        state
            .content_indices
            .insert(canonical_root.to_string_lossy().to_string(), index_arc);

        let r = dispatch(
            &state,
            "s1",
            "search_code",
            &json!({ "query": "authentication" }),
        )
        .await;
        assert!(r.success, "{}", r.output);
        let parsed: Value = serde_json::from_str(&r.output).unwrap();
        let results = parsed["results"].as_array().unwrap();
        assert!(!results.is_empty(), "expected at least one result");
        assert!(
            results[0]["path"].as_str().unwrap().contains("auth.rs"),
            "auth.rs should rank highest for 'authentication'"
        );
        assert!(results[0]["score"].as_f64().unwrap() > 0.0);
        assert!(!results[0]["snippet"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn search_code_not_ready_returns_empty_with_note() {
        let (dir, state) = fs_test_state("s1");
        let repo_root = dir.path().to_path_buf();

        // Insert an empty (not-yet-built) index
        let index_arc = Arc::new(parking_lot::RwLock::new(
            crate::content_index::ContentIndex::empty(repo_root.clone()),
        ));
        state
            .content_indices
            .insert(repo_root.to_string_lossy().to_string(), index_arc);

        let r = dispatch(&state, "s1", "search_code", &json!({ "query": "anything" })).await;
        assert!(r.success, "{}", r.output);
        let parsed: Value = serde_json::from_str(&r.output).unwrap();
        assert_eq!(parsed["results"].as_array().unwrap().len(), 0);
        assert!(parsed["note"].as_str().is_some());
    }

    // ── search_tools ───────────────────────────────────────────

    #[tokio::test]
    async fn search_tools_returns_empty_when_no_upstreams() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let r = dispatch(&state, "s1", "search_tools", &json!({})).await;
        assert!(r.success, "{}", r.output);
        let parsed: Value = serde_json::from_str(&r.output).unwrap();
        let tools = parsed["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 0);
        assert_eq!(parsed["count"], 0);
    }

    #[tokio::test]
    async fn search_tools_response_has_required_fields() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let r = dispatch(
            &state,
            "s1",
            "search_tools",
            &json!({ "query": "jira", "limit": 5 }),
        )
        .await;
        assert!(r.success, "{}", r.output);
        let parsed: Value = serde_json::from_str(&r.output).unwrap();
        assert!(parsed["tools"].is_array());
        assert!(parsed["count"].is_number());
    }

    // ── call_tool ──────────────────────────────────────────────

    #[tokio::test]
    async fn call_tool_missing_tool_name_returns_error() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let r = dispatch(&state, "s1", "call_tool", &json!({})).await;
        assert!(!r.success);
        assert!(r.output.contains("tool_name"));
    }

    #[tokio::test]
    async fn call_tool_unknown_upstream_returns_error() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let r = dispatch(
            &state,
            "s1",
            "call_tool",
            &json!({ "tool_name": "no_such__tool" }),
        )
        .await;
        assert!(!r.success);
        assert!(
            !r.output.contains("Unknown tool:"),
            "should route call_tool, not fall through to unknown-tool handler"
        );
    }

    // ── schedule_task validation ────────────────────────────────

    #[test]
    fn schedule_task_rejects_missing_goal() {
        let r = exec_schedule_task(&json!({ "interval_minutes": 10 }));
        assert!(!r.success);
        assert!(r.output.contains("goal"));
    }

    #[test]
    fn schedule_task_rejects_goal_too_long() {
        let long_goal = "x".repeat(501);
        let r = exec_schedule_task(&json!({ "goal": long_goal, "interval_minutes": 10 }));
        assert!(!r.success);
        assert!(r.output.contains("500"));
    }

    #[test]
    fn schedule_task_rejects_interval_below_5() {
        let r = exec_schedule_task(&json!({ "goal": "do something", "interval_minutes": 4 }));
        assert!(!r.success);
        assert!(r.output.contains("at least 5"));
    }

    #[test]
    fn schedule_task_rejects_missing_interval() {
        let r = exec_schedule_task(&json!({ "goal": "do something" }));
        assert!(!r.success);
        assert!(r.output.contains("interval_minutes"));
    }

    #[test]
    fn cancel_schedule_rejects_missing_id() {
        let r = exec_cancel_schedule(&json!({}));
        assert!(!r.success);
        assert!(r.output.contains("id"));
    }

    #[test]
    fn cancel_schedule_rejects_nonexistent_id() {
        let r = exec_cancel_schedule(&json!({ "id": "nonexistent-job-xyz" }));
        assert!(!r.success);
    }

    // ── watch_for / list_watches / cancel_watch ─────────────────

    #[tokio::test]
    async fn watch_for_requires_approval_to_arm() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let r = dispatch(
            &state,
            "ws-1",
            "watch_for",
            &json!({"trigger": {"type": "error"}, "instructions": "investigate"}),
        )
        .await;
        assert!(r.needs_approval, "arming a watch must require approval");
    }

    #[tokio::test]
    async fn watch_for_rejects_invalid_trigger() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let r = dispatch(
            &state,
            "ws-2",
            "watch_for",
            &json!({"trigger": {"type": "nonsense"}, "instructions": "x"}),
        )
        .await;
        assert!(!r.success);
        assert!(r.output.contains("Invalid trigger"), "got: {}", r.output);
    }

    #[tokio::test]
    async fn watch_for_rejects_missing_instructions() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let r = dispatch(
            &state,
            "ws-3",
            "watch_for",
            &json!({"trigger": {"type": "idle"}}),
        )
        .await;
        assert!(!r.success);
        assert!(r.output.contains("instructions"), "got: {}", r.output);
    }

    #[tokio::test]
    async fn watch_lifecycle_arm_list_cancel_scoped_to_session() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let engine = Arc::new(super::super::watcher::WatcherEngine::new(state.clone()));
        let _ = state.watcher_engine.set(engine);

        // Arm via the approved path (skips safety, as the user just approved).
        let armed = dispatch_approved(
            &state,
            "wl-1",
            "watch_for",
            &json!({
                "trigger": {"type": "command_done", "on_failure_only": true},
                "instructions": "summarize the failure",
                "name": "fail-watch"
            }),
        )
        .await;
        assert!(armed.success, "arm failed: {}", armed.output);
        let armed_json: Value = serde_json::from_str(&armed.output).unwrap();
        assert_eq!(armed_json["status"], "armed");
        let watch_id = armed_json["watch_id"].as_str().unwrap().to_string();

        // Reachable via engine.config(), scoped to this session.
        {
            let cfg = state.watcher_engine.get().unwrap().config();
            let config = cfg.read();
            assert!(
                config
                    .rules
                    .iter()
                    .any(|r| r.id == watch_id && r.session_id.as_deref() == Some("wl-1")),
                "armed rule not found in config for session wl-1"
            );
        }

        // list_watches returns it for the owning session.
        let listed = dispatch(&state, "wl-1", "list_watches", &json!({})).await;
        assert!(listed.success);
        let listed_json: Value = serde_json::from_str(&listed.output).unwrap();
        let watches = listed_json["watches"].as_array().unwrap();
        assert_eq!(watches.len(), 1);
        assert_eq!(watches[0]["watch_id"], watch_id);

        // A different session sees none of wl-1's watches.
        let other = dispatch(&state, "wl-other", "list_watches", &json!({})).await;
        let other_json: Value = serde_json::from_str(&other.output).unwrap();
        assert_eq!(other_json["watches"].as_array().unwrap().len(), 0);

        // A foreign session cannot cancel wl-1's watch.
        let bad = dispatch(
            &state,
            "wl-other",
            "cancel_watch",
            &json!({"watch_id": watch_id}),
        )
        .await;
        assert!(!bad.success, "foreign session must not cancel the watch");

        // The owner cancels it.
        let cancelled = dispatch(
            &state,
            "wl-1",
            "cancel_watch",
            &json!({"watch_id": watch_id}),
        )
        .await;
        assert!(cancelled.success, "cancel failed: {}", cancelled.output);

        // Gone afterwards.
        let after = dispatch(&state, "wl-1", "list_watches", &json!({})).await;
        let after_json: Value = serde_json::from_str(&after.output).unwrap();
        assert_eq!(after_json["watches"].as_array().unwrap().len(), 0);
    }

    // ── search_scrollback ────────────────────────────────────────

    #[tokio::test]
    async fn search_scrollback_returns_matches_with_offsets() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut vt = crate::state::VtLogBuffer::new(24, 80, 10_000);
        vt.process(b"the quick brown fox\r\njumped over fox tracks\r\n");
        state
            .vt_log_buffers
            .insert("sb-1".to_string(), parking_lot::Mutex::new(vt));

        let r = dispatch(
            &state,
            "sb-1",
            "search_scrollback",
            &json!({"session_id": "sb-1", "query": "fox"}),
        )
        .await;
        assert!(r.success, "search failed: {}", r.output);
        let json: Value = serde_json::from_str(&r.output).unwrap();
        let matches = json["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 2, "expected two 'fox' matches");
        for m in matches {
            assert!(m["line_index"].is_number());
            assert!(m["line_text"].as_str().unwrap().contains("fox"));
            assert!(m["match_end"].as_u64().unwrap() > m["match_start"].as_u64().unwrap());
        }
    }

    #[tokio::test]
    async fn search_scrollback_missing_buffer_errors() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let r = dispatch(
            &state,
            "sb-none",
            "search_scrollback",
            &json!({"session_id": "sb-none", "query": "x"}),
        )
        .await;
        assert!(!r.success);
        assert!(r.output.contains("No VT buffer"), "got: {}", r.output);
    }

    #[tokio::test]
    async fn search_scrollback_respects_limit() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut vt = crate::state::VtLogBuffer::new(24, 80, 10_000);
        vt.process(b"match\r\nmatch\r\nmatch\r\n");
        state
            .vt_log_buffers
            .insert("sb-lim".to_string(), parking_lot::Mutex::new(vt));

        let r = dispatch(
            &state,
            "sb-lim",
            "search_scrollback",
            &json!({"session_id": "sb-lim", "query": "match", "limit": 2}),
        )
        .await;
        assert!(r.success);
        let json: Value = serde_json::from_str(&r.output).unwrap();
        assert_eq!(json["matches"].as_array().unwrap().len(), 2);
    }

    // ── get_hyperlinks ───────────────────────────────────────────

    #[tokio::test]
    async fn get_hyperlinks_returns_osc8_links() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut vt = crate::state::VtLogBuffer::new(24, 80, 10_000);
        // OSC 8 hyperlink: ESC ] 8 ; ; URI BEL  text  ESC ] 8 ; ; BEL
        vt.process(b"\x1b]8;;https://example.com/page\x07click here\x1b]8;;\x07\r\n");
        state
            .vt_log_buffers
            .insert("hl-1".to_string(), parking_lot::Mutex::new(vt));

        let r = dispatch(
            &state,
            "hl-1",
            "get_hyperlinks",
            &json!({"session_id": "hl-1"}),
        )
        .await;
        assert!(r.success, "get_hyperlinks failed: {}", r.output);
        let json: Value = serde_json::from_str(&r.output).unwrap();
        let links = json["hyperlinks"].as_array().unwrap();
        assert_eq!(links.len(), 1, "expected one coalesced link");
        assert!(links[0]["uri"].as_str().unwrap().contains("example.com"));
        assert!(
            links[0]["end_col"].as_u64().unwrap() > links[0]["start_col"].as_u64().unwrap(),
            "span must be non-empty"
        );
    }

    #[tokio::test]
    async fn get_hyperlinks_empty_when_none() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut vt = crate::state::VtLogBuffer::new(24, 80, 10_000);
        vt.process(b"no links here\r\n");
        state
            .vt_log_buffers
            .insert("hl-2".to_string(), parking_lot::Mutex::new(vt));

        let r = dispatch(
            &state,
            "hl-2",
            "get_hyperlinks",
            &json!({"session_id": "hl-2"}),
        )
        .await;
        assert!(r.success);
        let json: Value = serde_json::from_str(&r.output).unwrap();
        assert_eq!(json["hyperlinks"].as_array().unwrap().len(), 0);
    }

    // ── get_semantic_zones ───────────────────────────────────────

    #[tokio::test]
    async fn get_semantic_zones_groups_prompt_input_output() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut vt = crate::state::VtLogBuffer::new(24, 80, 10_000);
        // OSC 133: A=prompt, B=input (command), C=output.
        vt.process(b"\x1b]133;A\x07$ \x1b]133;B\x07ls -la\x1b]133;C\x07file1.txt");
        state
            .vt_log_buffers
            .insert("sz-1".to_string(), parking_lot::Mutex::new(vt));

        let r = dispatch(
            &state,
            "sz-1",
            "get_semantic_zones",
            &json!({"session_id": "sz-1"}),
        )
        .await;
        assert!(r.success, "get_semantic_zones failed: {}", r.output);
        let json: Value = serde_json::from_str(&r.output).unwrap();
        let zones = json["zones"].as_array().unwrap();
        let kinds: Vec<&str> = zones.iter().map(|z| z["kind"].as_str().unwrap()).collect();
        assert!(kinds.contains(&"prompt"), "kinds: {kinds:?}");
        assert!(kinds.contains(&"input"), "kinds: {kinds:?}");
        assert!(kinds.contains(&"output"), "kinds: {kinds:?}");
        let output_zone = zones.iter().find(|z| z["kind"] == "output").unwrap();
        assert!(output_zone["text"].as_str().unwrap().contains("file1.txt"));
    }

    #[tokio::test]
    async fn get_semantic_zones_empty_for_plain_text() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut vt = crate::state::VtLogBuffer::new(24, 80, 10_000);
        vt.process(b"plain output, no osc133\r\n");
        state
            .vt_log_buffers
            .insert("sz-2".to_string(), parking_lot::Mutex::new(vt));

        let r = dispatch(
            &state,
            "sz-2",
            "get_semantic_zones",
            &json!({"session_id": "sz-2"}),
        )
        .await;
        assert!(r.success);
        let json: Value = serde_json::from_str(&r.output).unwrap();
        assert_eq!(json["zones"].as_array().unwrap().len(), 0);
    }
}
