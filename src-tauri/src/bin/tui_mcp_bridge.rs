/// MCP (Model Context Protocol) bridge binary for TUI Commander.
/// Translates MCP JSON-RPC 2.0 over stdio into HTTP calls to the Tauri app's local API.
/// Designed for consumption by Claude Code, Cursor, and other MCP-capable tools.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, BufRead, Write};

// --- MCP Protocol Types ---

#[derive(Deserialize)]
#[allow(dead_code)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

#[derive(Serialize)]
struct ToolDefinition {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: Value,
}

// --- Special Key Translation ---

fn translate_special_key(key: &str) -> Option<&'static str> {
    match key {
        "enter" | "return" => Some("\r"),
        "tab" => Some("\t"),
        "escape" | "esc" => Some("\x1b"),
        "backspace" => Some("\x7f"),
        "delete" => Some("\x1b[3~"),
        "up" => Some("\x1b[A"),
        "down" => Some("\x1b[B"),
        "right" => Some("\x1b[C"),
        "left" => Some("\x1b[D"),
        "home" => Some("\x1b[H"),
        "end" => Some("\x1b[F"),
        "ctrl+c" => Some("\x03"),
        "ctrl+d" => Some("\x04"),
        "ctrl+z" => Some("\x1a"),
        "ctrl+l" => Some("\x0c"),
        "ctrl+a" => Some("\x01"),
        "ctrl+e" => Some("\x05"),
        "ctrl+k" => Some("\x0b"),
        "ctrl+u" => Some("\x15"),
        "ctrl+w" => Some("\x17"),
        "ctrl+r" => Some("\x12"),
        "ctrl+p" => Some("\x10"),
        "ctrl+n" => Some("\x0e"),
        _ => None,
    }
}

// --- Tool Definitions (5 meta-commands) ---

fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "session".into(),
            description: "Manage PTY terminal sessions.\n\n\
                Actions (pass as 'action' parameter):\n\
                - list: Returns [{session_id, cwd, worktree_path, worktree_branch}] for all active sessions. Call first to discover IDs.\n\
                - create: Creates a new PTY session. Returns {session_id}. Optional: rows, cols, shell, cwd.\n\
                - input: Sends text and/or a special key to a session. Requires session_id, plus input and/or special_key.\n\
                - output: Returns {data, total_written} from session ring buffer. Requires session_id. Optional: limit.\n\
                - resize: Resizes PTY dimensions. Requires session_id, rows, cols.\n\
                - close: Terminates a session. Requires session_id.\n\
                - pause: Pauses output buffering. Requires session_id.\n\
                - resume: Resumes output buffering. Requires session_id.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "description": "One of: list, create, input, output, resize, close, pause, resume" },
                    "session_id": { "type": "string", "description": "Session ID (required for input, output, resize, close, pause, resume)" },
                    "input": { "type": "string", "description": "Raw text to write (action=input)" },
                    "special_key": { "type": "string", "description": "Special key: enter, tab, ctrl+c, ctrl+d, ctrl+z, ctrl+l, ctrl+a, ctrl+e, ctrl+k, ctrl+u, ctrl+w, ctrl+r, up, down, left, right, home, end, backspace, delete, escape (action=input)" },
                    "rows": { "type": "integer", "description": "Terminal rows (action=create or resize)" },
                    "cols": { "type": "integer", "description": "Terminal cols (action=create or resize)" },
                    "shell": { "type": "string", "description": "Shell binary path (action=create)" },
                    "cwd": { "type": "string", "description": "Working directory (action=create)" },
                    "limit": { "type": "integer", "description": "Bytes to read, default 8192 (action=output)" }
                },
                "required": ["action"]
            }),
        },
        ToolDefinition {
            name: "git".into(),
            description: "Query git repository state and GitHub integration.\n\n\
                Actions (pass as 'action' parameter):\n\
                - info: Returns {name, branch, status, remote_url, is_dirty, ahead, behind}. Requires path.\n\
                - diff: Returns {diff} with unified diff of unstaged changes. Requires path.\n\
                - files: Returns [{path, status, insertions, deletions}] for changed files. Requires path.\n\
                - branches: Returns [{name, is_current, is_remote}] branch list. Requires path.\n\
                - github: Returns GitHub integration data (remote, PR, CI, ahead/behind). Requires path.\n\
                - prs: Returns all open PR statuses with CI rollup. Requires path.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "description": "One of: info, diff, files, branches, github, prs" },
                    "path": { "type": "string", "description": "Absolute path to git repository (required for all actions)" }
                },
                "required": ["action"]
            }),
        },
        ToolDefinition {
            name: "agent".into(),
            description: "Detect and manage AI agents.\n\n\
                Actions (pass as 'action' parameter):\n\
                - detect: Returns [{name, path, version}] for known agents (claude, codex, aider, goose, lazygit).\n\
                - spawn: Launches an agent in a new PTY session. Requires prompt. Returns {session_id}. Use session action=input/output to interact.\n\
                - stats: Returns {active_sessions, max_sessions, available_slots}.\n\
                - metrics: Returns cumulative metrics {total_spawned, total_failed, active_sessions, bytes_emitted, pauses_triggered}.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "description": "One of: detect, spawn, stats, metrics" },
                    "prompt": { "type": "string", "description": "Task prompt for the agent (action=spawn)" },
                    "cwd": { "type": "string", "description": "Working directory (action=spawn)" },
                    "model": { "type": "string", "description": "Model override (action=spawn)" },
                    "print_mode": { "type": "boolean", "description": "Non-interactive mode (action=spawn)" },
                    "output_format": { "type": "string", "description": "Output format, e.g. 'json' (action=spawn)" },
                    "agent_type": { "type": "string", "description": "Agent binary: claude, codex, aider, goose (action=spawn)" },
                    "binary_path": { "type": "string", "description": "Override agent binary path (action=spawn)" },
                    "args": { "type": "array", "items": { "type": "string" }, "description": "Raw CLI args (action=spawn)" },
                    "rows": { "type": "integer", "description": "Terminal rows (action=spawn)" },
                    "cols": { "type": "integer", "description": "Terminal cols (action=spawn)" }
                },
                "required": ["action"]
            }),
        },
        ToolDefinition {
            name: "config".into(),
            description: "Read or write app configuration.\n\n\
                Actions (pass as 'action' parameter):\n\
                - get: Returns app config (shell, font, theme, worktree_dir, etc.). Password hash is stripped.\n\
                - save: Persists configuration. Requires config object. Partial updates OK.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "description": "One of: get, save" },
                    "config": { "type": "object", "description": "Config fields to save (action=save)" }
                },
                "required": ["action"]
            }),
        },
        ToolDefinition {
            name: "plugin_dev_guide".into(),
            description: "Returns comprehensive plugin authoring reference: manifest format, PluginHost API (all 4 tiers), structured event types, and working examples. Call before writing any plugin code.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
    ]
}

// --- HTTP Client ---

struct HttpClient {
    base_url: String,
    client: reqwest::blocking::Client,
}

impl HttpClient {
    fn new(port: u16) -> Self {
        Self {
            base_url: format!("http://127.0.0.1:{}", port),
            client: reqwest::blocking::Client::new(),
        }
    }

    fn get(&self, path: &str) -> Result<Value, String> {
        let resp = self
            .client
            .get(format!("{}{}", self.base_url, path))
            .send()
            .map_err(|e| format!("HTTP request failed: {}", e))?;
        resp.json::<Value>()
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    fn post(&self, path: &str, body: &Value) -> Result<Value, String> {
        let resp = self
            .client
            .post(format!("{}{}", self.base_url, path))
            .json(body)
            .send()
            .map_err(|e| format!("HTTP request failed: {}", e))?;
        resp.json::<Value>()
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    fn put(&self, path: &str, body: &Value) -> Result<Value, String> {
        let resp = self
            .client
            .put(format!("{}{}", self.base_url, path))
            .json(body)
            .send()
            .map_err(|e| format!("HTTP request failed: {}", e))?;
        resp.json::<Value>()
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    fn delete(&self, path: &str) -> Result<Value, String> {
        let resp = self
            .client
            .delete(format!("{}{}", self.base_url, path))
            .send()
            .map_err(|e| format!("HTTP request failed: {}", e))?;
        resp.json::<Value>()
            .map_err(|e| format!("Failed to parse response: {}", e))
    }
}

// --- Tool Call Handlers ---

fn handle_tool_call(client: &HttpClient, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "session" => handle_session(client, args),
        "git" => handle_git(client, args),
        "agent" => handle_agent(client, args),
        "config" => handle_config(client, args),
        "plugin_dev_guide" => client.get("/plugins/docs"),
        _ => Err(format!(
            "Unknown tool '{}'. Available: session, git, agent, config, plugin_dev_guide",
            name
        )),
    }
}

fn require_action(args: &Value, tool: &str, available: &str) -> Result<String, String> {
    args["action"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Missing 'action'. Available actions for '{}': {}", tool, available))
}

fn require_session_id(args: &Value, action: &str) -> Result<String, String> {
    args["session_id"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!(
            "Action '{}' requires 'session_id'. Get valid IDs with session action='list'",
            action
        ))
}

fn require_path(args: &Value, action: &str) -> Result<String, String> {
    args["path"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!(
            "Action '{}' requires 'path' (absolute path to git repository)",
            action
        ))
}

const SESSION_ACTIONS: &str = "list, create, input, output, resize, close, pause, resume";
const GIT_ACTIONS: &str = "info, diff, files, branches, github, prs";
const AGENT_ACTIONS: &str = "detect, spawn, stats, metrics";
const CONFIG_ACTIONS: &str = "get, save";

fn handle_session(client: &HttpClient, args: &Value) -> Result<Value, String> {
    let action = require_action(args, "session", SESSION_ACTIONS)?;
    match action.as_str() {
        "list" => client.get("/sessions"),

        "create" => {
            let body = serde_json::json!({
                "rows": args["rows"].as_u64().unwrap_or(24),
                "cols": args["cols"].as_u64().unwrap_or(80),
                "shell": args["shell"].as_str(),
                "cwd": args["cwd"].as_str(),
            });
            client.post("/sessions", &body)
        }

        "input" => {
            let session_id = require_session_id(args, "input")?;
            let mut data = String::new();
            if let Some(input) = args["input"].as_str() {
                data.push_str(input);
            }
            if let Some(key) = args["special_key"].as_str() {
                match translate_special_key(key) {
                    Some(seq) => data.push_str(seq),
                    None => return Err(format!("Unknown special key: {}", key)),
                }
            }
            if data.is_empty() {
                return Err("Action 'input' requires 'input' (text) and/or 'special_key'".into());
            }
            client.post(
                &format!("/sessions/{}/write", session_id),
                &serde_json::json!({"data": data}),
            )
        }

        "output" => {
            let session_id = require_session_id(args, "output")?;
            let limit = args["limit"].as_u64().unwrap_or(8192);
            client.get(&format!("/sessions/{}/output?limit={}", session_id, limit))
        }

        "resize" => {
            let session_id = require_session_id(args, "resize")?;
            let rows = args["rows"].as_u64().ok_or(
                "Action 'resize' requires 'rows'. Also provide 'cols'."
            )?;
            let cols = args["cols"].as_u64().ok_or(
                "Action 'resize' requires 'cols'. Also provide 'rows'."
            )?;
            client.post(
                &format!("/sessions/{}/resize", session_id),
                &serde_json::json!({"rows": rows, "cols": cols}),
            )
        }

        "close" => {
            let session_id = require_session_id(args, "close")?;
            client.delete(&format!("/sessions/{}", session_id))
        }

        "pause" => {
            let session_id = require_session_id(args, "pause")?;
            client.post(
                &format!("/sessions/{}/pause", session_id),
                &serde_json::json!({}),
            )
        }

        "resume" => {
            let session_id = require_session_id(args, "resume")?;
            client.post(
                &format!("/sessions/{}/resume", session_id),
                &serde_json::json!({}),
            )
        }

        other => Err(format!(
            "Unknown action '{}' for tool 'session'. Available: {}",
            other, SESSION_ACTIONS
        )),
    }
}

fn handle_git(client: &HttpClient, args: &Value) -> Result<Value, String> {
    let action = require_action(args, "git", GIT_ACTIONS)?;
    match action.as_str() {
        "info" => {
            let path = require_path(args, "info")?;
            client.get(&format!("/repo/info?path={}", urlencoded(&path)))
        }
        "diff" => {
            let path = require_path(args, "diff")?;
            client.get(&format!("/repo/diff?path={}", urlencoded(&path)))
        }
        "files" => {
            let path = require_path(args, "files")?;
            client.get(&format!("/repo/files?path={}", urlencoded(&path)))
        }
        "branches" => {
            let path = require_path(args, "branches")?;
            client.get(&format!("/repo/branches?path={}", urlencoded(&path)))
        }
        "github" => {
            let path = require_path(args, "github")?;
            client.get(&format!("/repo/github?path={}", urlencoded(&path)))
        }
        "prs" => {
            let path = require_path(args, "prs")?;
            client.get(&format!("/repo/prs?path={}", urlencoded(&path)))
        }
        other => Err(format!(
            "Unknown action '{}' for tool 'git'. Available: {}",
            other, GIT_ACTIONS
        )),
    }
}

fn handle_agent(client: &HttpClient, args: &Value) -> Result<Value, String> {
    let action = require_action(args, "agent", AGENT_ACTIONS)?;
    match action.as_str() {
        "detect" => client.get("/agents"),
        "spawn" => {
            let prompt = args["prompt"]
                .as_str()
                .ok_or("Action 'spawn' requires 'prompt'")?;
            let body = serde_json::json!({
                "prompt": prompt,
                "cwd": args["cwd"].as_str(),
                "model": args["model"].as_str(),
                "print_mode": args["print_mode"].as_bool(),
                "output_format": args["output_format"].as_str(),
                "agent_type": args["agent_type"].as_str(),
                "binary_path": args["binary_path"].as_str(),
                "args": args.get("args"),
                "rows": args["rows"].as_u64().unwrap_or(24),
                "cols": args["cols"].as_u64().unwrap_or(80),
            });
            client.post("/sessions/agent", &body)
        }
        "stats" => client.get("/stats"),
        "metrics" => client.get("/metrics"),
        other => Err(format!(
            "Unknown action '{}' for tool 'agent'. Available: {}",
            other, AGENT_ACTIONS
        )),
    }
}

fn handle_config(client: &HttpClient, args: &Value) -> Result<Value, String> {
    let action = require_action(args, "config", CONFIG_ACTIONS)?;
    match action.as_str() {
        "get" => client.get("/config"),
        "save" => {
            let config = args.get("config").ok_or(
                "Action 'save' requires 'config' object"
            )?;
            client.put("/config", config)
        }
        other => Err(format!(
            "Unknown action '{}' for tool 'config'. Available: {}",
            other, CONFIG_ACTIONS
        )),
    }
}

/// Minimal URL encoding for query parameter values
fn urlencoded(s: &str) -> String {
    s.replace('%', "%25")
        .replace(' ', "%20")
        .replace('#', "%23")
        .replace('&', "%26")
        .replace('?', "%3F")
        .replace('=', "%3D")
}

// --- Main ---

fn config_dir() -> std::path::PathBuf {
    dirs::config_dir()
        .map(|d| d.join("tuicommander"))
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".tuicommander")
        })
}

fn read_port() -> Result<u16, String> {
    let port_file = config_dir().join("mcp-port");

    let contents = std::fs::read_to_string(&port_file).map_err(|e| {
        format!(
            "Cannot read port file at {}: {}. Is TUI Commander running with MCP enabled?",
            port_file.display(),
            e
        )
    })?;

    contents
        .trim()
        .parse::<u16>()
        .map_err(|e| format!("Invalid port in {}: {}", port_file.display(), e))
}

/// Write a JSON-RPC response to stdout.
/// Uses LF (\n) line endings on all platforms — this is intentional.
/// JSON-RPC 2.0 over stdio uses \n as the message delimiter; MCP clients
/// (Claude Code, Cursor, etc.) expect \n even on Windows.
fn send_response(resp: &JsonRpcResponse) {
    let json = match serde_json::to_string(resp) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("tui-mcp-bridge: failed to serialize response: {e}");
            // Send a minimal error response that we know can serialize
            r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"Internal serialization error"}}"#.to_string()
        }
    };
    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{}", json);
    let _ = stdout.flush();
}

fn main() {
    let port = match read_port() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("tui-mcp-bridge: {}", e);
            std::process::exit(1);
        }
    };

    let client = HttpClient::new(port);

    // Verify connection
    match client.get("/health") {
        Ok(_) => eprintln!("tui-mcp-bridge: connected to TUI Commander on port {}", port),
        Err(e) => {
            eprintln!("tui-mcp-bridge: cannot connect to TUI Commander: {}", e);
            std::process::exit(1);
        }
    }

    let stdin = io::stdin();
    let reader = stdin.lock();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break, // stdin closed
        };

        if line.trim().is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("tui-mcp-bridge: invalid JSON-RPC: {}", e);
                continue;
            }
        };

        let id = request.id.clone().unwrap_or(Value::Null);

        match request.method.as_str() {
            "initialize" => {
                send_response(&JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id,
                    result: Some(serde_json::json!({
                        "protocolVersion": "2024-11-05",
                        "capabilities": {
                            "tools": {}
                        },
                        "serverInfo": {
                            "name": "tuicommander",
                            "version": env!("CARGO_PKG_VERSION")
                        }
                    })),
                    error: None,
                });
            }

            "notifications/initialized" => {
                // Client acknowledgment, no response needed
            }

            "tools/list" => {
                let tools = tool_definitions();
                send_response(&JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id,
                    result: Some(serde_json::json!({ "tools": tools })),
                    error: None,
                });
            }

            "tools/call" => {
                let params = request.params.unwrap_or(Value::Null);
                let tool_name = params["name"].as_str().unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(Value::Object(Default::default()));

                match handle_tool_call(&client, tool_name, &args) {
                    Ok(result) => {
                        let text = serde_json::to_string_pretty(&result).unwrap_or_default();
                        send_response(&JsonRpcResponse {
                            jsonrpc: "2.0".into(),
                            id,
                            result: Some(serde_json::json!({
                                "content": [{
                                    "type": "text",
                                    "text": text
                                }]
                            })),
                            error: None,
                        });
                    }
                    Err(e) => {
                        send_response(&JsonRpcResponse {
                            jsonrpc: "2.0".into(),
                            id,
                            result: Some(serde_json::json!({
                                "content": [{
                                    "type": "text",
                                    "text": format!("Error: {}", e)
                                }],
                                "isError": true
                            })),
                            error: None,
                        });
                    }
                }
            }

            other => {
                send_response(&JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32601,
                        message: format!("Method not found: {}", other),
                    }),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_special_key_translation() {
        assert_eq!(translate_special_key("enter"), Some("\r"));
        assert_eq!(translate_special_key("ctrl+c"), Some("\x03"));
        assert_eq!(translate_special_key("tab"), Some("\t"));
        assert_eq!(translate_special_key("up"), Some("\x1b[A"));
        assert_eq!(translate_special_key("unknown"), None);
    }

    #[test]
    fn test_tool_definitions_count_and_names() {
        let tools = tool_definitions();
        assert_eq!(tools.len(), 5);

        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"session"));
        assert!(names.contains(&"git"));
        assert!(names.contains(&"agent"));
        assert!(names.contains(&"config"));
        assert!(names.contains(&"plugin_dev_guide"));
    }

    #[test]
    fn test_tool_definitions_have_valid_schemas() {
        let tools = tool_definitions();
        for tool in &tools {
            assert!(
                tool.input_schema.is_object(),
                "Tool '{}' has non-object input_schema",
                tool.name
            );
            assert_eq!(
                tool.input_schema["type"], "object",
                "Tool '{}' input_schema type is not 'object'",
                tool.name
            );
        }
    }

    #[test]
    fn test_meta_commands_require_action() {
        let tools = tool_definitions();
        for tool in &tools {
            if tool.name == "plugin_dev_guide" {
                // No action required for single-purpose tool
                continue;
            }
            let required = tool.input_schema["required"].as_array().unwrap();
            assert!(
                required.iter().any(|v| v == "action"),
                "Tool '{}' should require 'action' parameter",
                tool.name
            );
        }
    }

    #[test]
    fn test_urlencoded() {
        assert_eq!(urlencoded("/tmp/my repo"), "/tmp/my%20repo");
        assert_eq!(urlencoded("path?q=1"), "path%3Fq%3D1");
        assert_eq!(urlencoded("a&b"), "a%26b");
    }
}
