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

// --- Tool Definitions ---

fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        // --- Session management ---
        ToolDefinition {
            name: "list_sessions".into(),
            description: "List all active terminal sessions with their IDs, working directories, and worktree info"
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "create_session".into(),
            description: "Create a new terminal session (PTY). Returns session_id for subsequent operations.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "rows": { "type": "integer", "description": "Terminal rows (default: 24)" },
                    "cols": { "type": "integer", "description": "Terminal columns (default: 80)" },
                    "shell": { "type": "string", "description": "Shell path (default: platform shell)" },
                    "cwd": { "type": "string", "description": "Working directory for the session" }
                },
                "required": []
            }),
        },
        ToolDefinition {
            name: "send_input".into(),
            description: "Send text or a special key to a terminal session. Use 'input' for text, 'special_key' for keys like 'enter', 'ctrl+c', 'tab', 'up', 'down', etc.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Terminal session ID" },
                    "input": { "type": "string", "description": "Text to type into the terminal" },
                    "special_key": { "type": "string", "description": "Special key: enter, tab, ctrl+c, ctrl+d, ctrl+z, ctrl+l, ctrl+a, ctrl+e, ctrl+k, ctrl+u, ctrl+w, ctrl+r, up, down, left, right, home, end, backspace, delete, escape" }
                },
                "required": ["session_id"]
            }),
        },
        ToolDefinition {
            name: "get_output".into(),
            description:
                "Read recent terminal output from a session's ring buffer (default 8KB, max 64KB)"
                    .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Terminal session ID" },
                    "limit": { "type": "integer", "description": "Max bytes to read (default 8192, max 65536)" }
                },
                "required": ["session_id"]
            }),
        },
        ToolDefinition {
            name: "resize_terminal".into(),
            description: "Resize a terminal session".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Terminal session ID" },
                    "rows": { "type": "integer", "description": "Number of rows" },
                    "cols": { "type": "integer", "description": "Number of columns" }
                },
                "required": ["session_id", "rows", "cols"]
            }),
        },
        ToolDefinition {
            name: "close_session".into(),
            description: "Close a terminal session. Sends Ctrl+C and waits briefly for graceful shutdown.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Terminal session ID" }
                },
                "required": ["session_id"]
            }),
        },
        ToolDefinition {
            name: "pause_session".into(),
            description: "Pause a terminal session's output reader (flow control)".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Terminal session ID" }
                },
                "required": ["session_id"]
            }),
        },
        ToolDefinition {
            name: "resume_session".into(),
            description: "Resume a paused terminal session's output reader (flow control)".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Terminal session ID" }
                },
                "required": ["session_id"]
            }),
        },
        // --- Orchestrator ---
        ToolDefinition {
            name: "get_stats".into(),
            description: "Get orchestrator stats: active sessions, max sessions, available slots".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "get_metrics".into(),
            description: "Get session metrics: total spawned, failed spawns, bytes emitted, pauses triggered".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        // --- Git/GitHub ---
        ToolDefinition {
            name: "get_repo_info".into(),
            description: "Get git repository info (branch, status, name) for a path".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Repository path" }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "get_git_diff".into(),
            description: "Get unified diff for a repository (unstaged changes)".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Repository path" }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "get_changed_files".into(),
            description: "Get list of changed files with status and per-file +/- stats".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Repository path" }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "get_github_status".into(),
            description: "Get GitHub status: remote info, current branch, PR status, CI status, ahead/behind counts".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Repository path" }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "get_pr_statuses".into(),
            description: "Get all PR statuses for a repository (branch, title, state, CI checks, review decision)".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Repository path" }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "get_branches".into(),
            description: "Get list of git branches (local and remote) with current branch indicator".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Repository path" }
                },
                "required": ["path"]
            }),
        },
        // --- Config ---
        ToolDefinition {
            name: "get_config".into(),
            description: "Get TUI Commander application configuration".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "save_config".into(),
            description: "Save TUI Commander application configuration".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "config": {
                        "type": "object",
                        "description": "Configuration object with fields: shell, font_family, font_size, theme, worktree_dir, mcp_server_enabled"
                    }
                },
                "required": ["config"]
            }),
        },
        // --- Agents ---
        ToolDefinition {
            name: "detect_agents".into(),
            description: "Detect installed AI agent binaries (claude, codex, aider, goose, lazygit)".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "spawn_agent".into(),
            description: "Spawn an AI agent in a new terminal session. Returns session_id to interact with the agent.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string", "description": "Prompt/task for the agent" },
                    "cwd": { "type": "string", "description": "Working directory" },
                    "model": { "type": "string", "description": "Model to use (if supported by agent)" },
                    "print_mode": { "type": "boolean", "description": "Use --print mode (non-interactive)" },
                    "output_format": { "type": "string", "description": "Output format (e.g., 'json')" },
                    "agent_type": { "type": "string", "description": "Agent binary name (default: claude)" },
                    "binary_path": { "type": "string", "description": "Explicit path to agent binary" },
                    "args": { "type": "array", "items": { "type": "string" }, "description": "Custom args (overrides default arg building)" },
                    "rows": { "type": "integer", "description": "Terminal rows (default: 24)" },
                    "cols": { "type": "integer", "description": "Terminal columns (default: 80)" }
                },
                "required": ["prompt"]
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
        "list_sessions" => client.get("/sessions"),

        "create_session" => {
            let body = serde_json::json!({
                "rows": args["rows"].as_u64().unwrap_or(24),
                "cols": args["cols"].as_u64().unwrap_or(80),
                "shell": args["shell"].as_str(),
                "cwd": args["cwd"].as_str(),
            });
            client.post("/sessions", &body)
        }

        "send_input" => {
            let session_id = args["session_id"]
                .as_str()
                .ok_or("missing session_id")?;

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
                return Err("Either 'input' or 'special_key' must be provided".into());
            }

            client.post(
                &format!("/sessions/{}/write", session_id),
                &serde_json::json!({"data": data}),
            )
        }

        "get_output" => {
            let session_id = args["session_id"]
                .as_str()
                .ok_or("missing session_id")?;
            let limit = args["limit"].as_u64().unwrap_or(8192);
            client.get(&format!(
                "/sessions/{}/output?limit={}",
                session_id, limit
            ))
        }

        "resize_terminal" => {
            let session_id = args["session_id"]
                .as_str()
                .ok_or("missing session_id")?;
            let rows = args["rows"].as_u64().ok_or("missing rows")?;
            let cols = args["cols"].as_u64().ok_or("missing cols")?;
            client.post(
                &format!("/sessions/{}/resize", session_id),
                &serde_json::json!({"rows": rows, "cols": cols}),
            )
        }

        "close_session" => {
            let session_id = args["session_id"]
                .as_str()
                .ok_or("missing session_id")?;
            client.delete(&format!("/sessions/{}", session_id))
        }

        "pause_session" => {
            let session_id = args["session_id"]
                .as_str()
                .ok_or("missing session_id")?;
            client.post(
                &format!("/sessions/{}/pause", session_id),
                &serde_json::json!({}),
            )
        }

        "resume_session" => {
            let session_id = args["session_id"]
                .as_str()
                .ok_or("missing session_id")?;
            client.post(
                &format!("/sessions/{}/resume", session_id),
                &serde_json::json!({}),
            )
        }

        "get_stats" => client.get("/stats"),

        "get_metrics" => client.get("/metrics"),

        "get_repo_info" => {
            let path = args["path"].as_str().ok_or("missing path")?;
            client.get(&format!("/repo/info?path={}", urlencoded(path)))
        }

        "get_git_diff" => {
            let path = args["path"].as_str().ok_or("missing path")?;
            client.get(&format!("/repo/diff?path={}", urlencoded(path)))
        }

        "get_changed_files" => {
            let path = args["path"].as_str().ok_or("missing path")?;
            client.get(&format!("/repo/files?path={}", urlencoded(path)))
        }

        "get_github_status" => {
            let path = args["path"].as_str().ok_or("missing path")?;
            client.get(&format!("/repo/github?path={}", urlencoded(path)))
        }

        "get_pr_statuses" => {
            let path = args["path"].as_str().ok_or("missing path")?;
            client.get(&format!("/repo/prs?path={}", urlencoded(path)))
        }

        "get_branches" => {
            let path = args["path"].as_str().ok_or("missing path")?;
            client.get(&format!("/repo/branches?path={}", urlencoded(path)))
        }

        "get_config" => client.get("/config"),

        "save_config" => {
            let config = args.get("config").ok_or("missing config")?;
            client.put("/config", config)
        }

        "detect_agents" => client.get("/agents"),

        "spawn_agent" => {
            let prompt = args["prompt"]
                .as_str()
                .ok_or("missing prompt")?;
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

        _ => Err(format!("Unknown tool: {}", name)),
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
        .map(|d| d.join("tui-commander"))
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".tui-commander")
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
                            "name": "tui-commander",
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
    fn test_tool_definitions_valid() {
        let tools = tool_definitions();
        assert_eq!(tools.len(), 20);

        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

        // Session management
        assert!(names.contains(&"list_sessions"));
        assert!(names.contains(&"create_session"));
        assert!(names.contains(&"send_input"));
        assert!(names.contains(&"get_output"));
        assert!(names.contains(&"resize_terminal"));
        assert!(names.contains(&"close_session"));
        assert!(names.contains(&"pause_session"));
        assert!(names.contains(&"resume_session"));

        // Orchestrator
        assert!(names.contains(&"get_stats"));
        assert!(names.contains(&"get_metrics"));

        // Git/GitHub
        assert!(names.contains(&"get_repo_info"));
        assert!(names.contains(&"get_git_diff"));
        assert!(names.contains(&"get_changed_files"));
        assert!(names.contains(&"get_github_status"));
        assert!(names.contains(&"get_pr_statuses"));
        assert!(names.contains(&"get_branches"));

        // Config
        assert!(names.contains(&"get_config"));
        assert!(names.contains(&"save_config"));

        // Agents
        assert!(names.contains(&"detect_agents"));
        assert!(names.contains(&"spawn_agent"));
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
    fn test_urlencoded() {
        assert_eq!(urlencoded("/tmp/my repo"), "/tmp/my%20repo");
        assert_eq!(urlencoded("path?q=1"), "path%3Fq%3D1");
        assert_eq!(urlencoded("a&b"), "a%26b");
    }
}
