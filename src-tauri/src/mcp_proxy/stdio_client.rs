//! Stdio MCP client — connects to an upstream MCP server by spawning a local process.
//!
//! Implements the MCP client side of the stdio transport:
//! newline-delimited JSON-RPC over stdin/stdout.
//!
//! - stderr from the child process goes to the app log (never parsed as protocol).
//! - Respawn is rate-limited to prevent tight loops on crashing servers.
//! - Env vars from the upstream config are merged with the inherited environment,
//!   so PATH and other vars work correctly in release builds launched from Finder.

use crate::mcp_proxy::http_client::UpstreamToolDef;
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::time::{Duration, Instant};

const MIN_RESPAWN_INTERVAL: Duration = Duration::from_secs(5);

/// Config for a stdio-based upstream MCP server.
#[derive(Debug, Clone)]
pub(crate) struct StdioConfig {
    pub(crate) name: String,
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    pub(crate) env: std::collections::HashMap<String, String>,
}

/// Client for a single upstream MCP server over stdio (spawned process).
pub(crate) struct StdioMcpClient {
    config: StdioConfig,
    /// Running child process (if connected).
    child: Option<std::process::Child>,
    /// Buffered reader over the child's stdout.
    stdout_reader: Option<BufReader<std::process::ChildStdout>>,
    /// Writable handle to the child's stdin.
    stdin: Option<std::process::ChildStdin>,
    /// When was the last spawn attempted (for rate limiting).
    last_spawn: Option<Instant>,
    /// JSON-RPC request counter.
    request_id: u64,
}

impl StdioMcpClient {
    /// Create a new stdio MCP client (not yet connected).
    pub(crate) fn new(config: StdioConfig) -> Self {
        Self {
            config,
            child: None,
            stdout_reader: None,
            stdin: None,
            last_spawn: None,
            request_id: 0,
        }
    }

    /// Build from an `UpstreamMcpServer` config.
    pub(crate) fn from_upstream_config(
        name: String,
        transport: &crate::mcp_upstream_config::UpstreamTransport,
    ) -> Option<Self> {
        match transport {
            crate::mcp_upstream_config::UpstreamTransport::Stdio { command, args, env } => {
                Some(Self::new(StdioConfig {
                    name,
                    command: command.clone(),
                    args: args.clone(),
                    env: env.clone(),
                }))
            }
            crate::mcp_upstream_config::UpstreamTransport::Http { .. } => None,
        }
    }

    /// Spawn the process and perform the MCP initialize handshake.
    /// Returns the tool definitions exposed by the server.
    pub(crate) fn spawn_and_initialize(&mut self) -> Result<Vec<UpstreamToolDef>, String> {
        // Rate limit: don't spawn more than once every MIN_RESPAWN_INTERVAL
        if let Some(last) = self.last_spawn {
            let elapsed = last.elapsed();
            if elapsed < MIN_RESPAWN_INTERVAL {
                return Err(format!(
                    "Upstream '{}' respawning too fast ({}ms since last spawn, min {}ms)",
                    self.config.name,
                    elapsed.as_millis(),
                    MIN_RESPAWN_INTERVAL.as_millis()
                ));
            }
        }

        // Tear down any previous process
        self.shutdown_internal();

        self.last_spawn = Some(Instant::now());

        // Build the command with a sanitized environment.
        // We clear the parent env to prevent credential leakage (ANTHROPIC_API_KEY,
        // AWS_SECRET_ACCESS_KEY, etc.) to potentially untrusted MCP server processes,
        // then re-add only the variables needed for normal operation.
        let mut cmd = std::process::Command::new(&self.config.command);
        cmd.args(&self.config.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit());

        cmd.env_clear();

        // Re-add safe passthrough variables needed by child processes
        const SAFE_ENV_KEYS: &[&str] = &[
            "PATH", "HOME", "USER", "LANG", "LC_ALL",
            "TMPDIR", "TEMP", "TMP", "SHELL", "TERM",
        ];
        for key in SAFE_ENV_KEYS {
            if let Ok(val) = std::env::var(key) {
                cmd.env(key, val);
            }
        }

        // Apply user-configured env overrides on top of the safe set
        for (k, v) in &self.config.env {
            cmd.env(k, v);
        }

        let mut child = cmd.spawn().map_err(|e| {
            format!(
                "Upstream '{}': failed to spawn '{}': {e}",
                self.config.name, self.config.command
            )
        })?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| format!("Upstream '{}': failed to get stdout", self.config.name))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| format!("Upstream '{}': failed to get stdin", self.config.name))?;

        self.stdout_reader = Some(BufReader::new(stdout));
        self.stdin = Some(stdin);
        self.child = Some(child);

        // MCP handshake
        let tools = self.do_initialize()?;
        Ok(tools)
    }

    /// Send the MCP initialize handshake and return tool definitions.
    fn do_initialize(&mut self) -> Result<Vec<UpstreamToolDef>, String> {
        // Send initialize
        let init_resp = self.rpc(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {
                    "name": "tuicommander",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )?;

        if init_resp.get("error").is_some() {
            return Err(format!(
                "Upstream '{}' initialize error: {}",
                self.config.name, init_resp["error"]
            ));
        }

        // Send notifications/initialized (fire-and-forget — no response expected)
        let _ = self.send_notification("notifications/initialized", serde_json::json!({}));

        // Fetch tool list
        let tools_resp = self.rpc("tools/list", serde_json::json!({}))?;
        let tools_arr = tools_resp["result"]["tools"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let tools = tools_arr
            .into_iter()
            .filter_map(|tool| {
                let original_name = tool["name"].as_str()?.to_string();
                Some(UpstreamToolDef {
                    original_name,
                    definition: tool,
                })
            })
            .collect();

        Ok(tools)
    }

    /// Call a tool on the upstream server.
    pub(crate) fn call_tool(&mut self, tool_name: &str, args: Value) -> Result<Value, String> {
        if !self.is_alive() {
            return Err(format!(
                "Upstream '{}' process is not running",
                self.config.name
            ));
        }
        let resp = self.rpc(
            "tools/call",
            serde_json::json!({
                "name": tool_name,
                "arguments": args
            }),
        )?;
        Ok(resp.get("result").cloned().unwrap_or(resp))
    }

    /// Check if the child process is still running.
    pub(crate) fn is_alive(&mut self) -> bool {
        match &mut self.child {
            None => false,
            Some(child) => {
                // try_wait returns None if still running, Some(status) if exited
                match child.try_wait() {
                    Ok(None) => true,           // still running
                    Ok(Some(_)) | Err(_) => {   // exited or error
                        self.child = None;
                        self.stdin = None;
                        self.stdout_reader = None;
                        false
                    }
                }
            }
        }
    }

    /// Gracefully shut down the child process.
    /// Closes stdin, waits up to 2s for voluntary exit, then kills.
    pub(crate) fn shutdown(&mut self) {
        self.shutdown_internal();
    }

    fn shutdown_internal(&mut self) {
        // Close stdin first — signals EOF to the child
        drop(self.stdin.take());
        drop(self.stdout_reader.take());

        if let Some(mut child) = self.child.take() {
            // Wait up to 2s for voluntary exit
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break, // exited voluntarily
                    Ok(None) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    _ => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                }
            }
        }
    }

    /// Send a JSON-RPC request and read the response.
    fn rpc(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.request_id += 1;
        let id = self.request_id;

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        self.write_line(&body)?;
        self.read_line()
    }

    /// Send a notification (no response expected).
    fn send_notification(&mut self, method: &str, params: Value) -> Result<(), String> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });
        self.write_line(&body)
    }

    /// Write a JSON value as a newline-delimited line to stdin.
    fn write_line(&mut self, value: &Value) -> Result<(), String> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| format!("Upstream '{}': stdin not available", self.config.name))?;

        let mut line = serde_json::to_string(value)
            .map_err(|e| format!("Upstream '{}': failed to serialize request: {e}", self.config.name))?;
        line.push('\n');

        stdin
            .write_all(line.as_bytes())
            .map_err(|e| format!("Upstream '{}': failed to write to stdin: {e}", self.config.name))?;
        stdin
            .flush()
            .map_err(|e| format!("Upstream '{}': failed to flush stdin: {e}", self.config.name))
    }

    /// Read a newline-delimited JSON response from stdout.
    fn read_line(&mut self) -> Result<Value, String> {
        let reader = self
            .stdout_reader
            .as_mut()
            .ok_or_else(|| format!("Upstream '{}': stdout not available", self.config.name))?;

        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|e| format!("Upstream '{}': failed to read from stdout: {e}", self.config.name))?;

        if line.is_empty() {
            return Err(format!(
                "Upstream '{}': server closed stdout (process may have crashed)",
                self.config.name
            ));
        }

        serde_json::from_str(line.trim()).map_err(|e| {
            format!(
                "Upstream '{}': invalid JSON from server: {e} (line: {line:?})",
                self.config.name
            )
        })
    }
}

impl Drop for StdioMcpClient {
    fn drop(&mut self) {
        self.shutdown_internal();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Create a test config that runs a simple echo-style MCP server
    /// implemented as a shell script.
    fn make_config_for_echo_server(script: &str) -> StdioConfig {
        // Write the script to a temp file
        let mut tmp = std::env::temp_dir();
        tmp.push(format!("tuic-mcp-test-{}.sh", uuid::Uuid::new_v4()));
        std::fs::write(&tmp, script).unwrap();
        // Make it executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        StdioConfig {
            name: "test".to_string(),
            command: "sh".to_string(),
            args: vec![tmp.to_str().unwrap().to_string()],
            env: HashMap::new(),
        }
    }

    /// A minimal MCP server script (shell) that responds correctly to the handshake.
    fn minimal_mcp_script() -> String {
        r#"#!/bin/sh
while IFS= read -r line; do
    method=$(echo "$line" | sed 's/.*"method":"\([^"]*\)".*/\1/')
    id=$(echo "$line" | sed 's/.*"id":\([0-9]*\).*/\1/')
    case "$method" in
        initialize)
            printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-03-26","capabilities":{"tools":{}},"serverInfo":{"name":"test","version":"1.0"}}}\n' "$id"
            ;;
        notifications/initialized)
            # No response for notifications
            ;;
        tools/list)
            printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"echo","description":"Echo tool","inputSchema":{"type":"object"}}]}}\n' "$id"
            ;;
        tools/call)
            printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"echoed"}],"isError":false}}\n' "$id"
            ;;
        *)
            printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32601,"message":"Method not found"}}\n' "$id"
            ;;
    esac
done
"#.to_string()
    }

    #[test]
    fn spawn_and_initialize_returns_tools() {
        let config = make_config_for_echo_server(&minimal_mcp_script());
        let mut client = StdioMcpClient::new(config);

        let tools = client.spawn_and_initialize().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].original_name, "echo");
        assert!(client.is_alive());

        client.shutdown();
        // After shutdown the process should be dead
        assert!(!client.is_alive());
    }

    #[test]
    fn call_tool_returns_result() {
        let config = make_config_for_echo_server(&minimal_mcp_script());
        let mut client = StdioMcpClient::new(config);
        client.spawn_and_initialize().unwrap();

        let result = client
            .call_tool("echo", serde_json::json!({"message": "hello"}))
            .unwrap();

        assert_eq!(result["content"][0]["text"].as_str().unwrap(), "echoed");
    }

    #[test]
    fn is_alive_returns_false_before_spawn() {
        let config = StdioConfig {
            name: "test".to_string(),
            command: "echo".to_string(),
            args: vec![],
            env: HashMap::new(),
        };
        let mut client = StdioMcpClient::new(config);
        assert!(!client.is_alive());
    }

    #[test]
    fn is_alive_returns_false_after_process_exits() {
        // A script that exits immediately after the handshake
        let script = r#"#!/bin/sh
IFS= read -r line
id=$(echo "$line" | sed 's/.*"id":\([0-9]*\).*/\1/')
printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-03-26","capabilities":{},"serverInfo":{"name":"test","version":"1.0"}}}\n' "$id"
IFS= read -r _notif
IFS= read -r line2
id2=$(echo "$line2" | sed 's/.*"id":\([0-9]*\).*/\1/')
printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[]}}\n' "$id2"
exit 0
"#;
        let config = make_config_for_echo_server(script);
        let mut client = StdioMcpClient::new(config);
        client.spawn_and_initialize().unwrap();

        // Give the process a moment to exit
        std::thread::sleep(Duration::from_millis(200));
        assert!(!client.is_alive());
    }

    #[test]
    fn spawn_fails_for_nonexistent_command() {
        let config = StdioConfig {
            name: "bad".to_string(),
            command: "this_command_does_not_exist_xyz_12345".to_string(),
            args: vec![],
            env: HashMap::new(),
        };
        let mut client = StdioMcpClient::new(config);
        let result = client.spawn_and_initialize();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("failed to spawn"));
    }

    #[test]
    fn respawn_rate_limit_blocks_too_fast_respawn() {
        let config = StdioConfig {
            name: "test".to_string(),
            command: "this_command_does_not_exist_xyz_12345".to_string(),
            args: vec![],
            env: HashMap::new(),
        };
        let mut client = StdioMcpClient::new(config);

        // First spawn attempt (will fail but updates last_spawn)
        // We manually set last_spawn to simulate a recent spawn
        client.last_spawn = Some(Instant::now());

        // Second attempt should be blocked by rate limiter
        let result = client.spawn_and_initialize();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("too fast") || err.contains("min"),
            "Expected rate limit error, got: {err}"
        );
    }

    #[test]
    fn shutdown_is_idempotent() {
        let config = make_config_for_echo_server(&minimal_mcp_script());
        let mut client = StdioMcpClient::new(config);
        client.spawn_and_initialize().unwrap();

        client.shutdown();
        client.shutdown(); // second call should not panic
        assert!(!client.is_alive());
    }

    #[test]
    fn env_vars_are_passed_to_child() {
        // Script that reads an env var and outputs it in the tool list
        let script = r#"#!/bin/sh
TEST_VAR_VALUE="$TUIC_TEST_ENV_VAR"
while IFS= read -r line; do
    method=$(echo "$line" | sed 's/.*"method":"\([^"]*\)".*/\1/')
    id=$(echo "$line" | sed 's/.*"id":\([0-9]*\).*/\1/')
    case "$method" in
        initialize)
            printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-03-26","capabilities":{},"serverInfo":{"name":"test","version":"1.0"}}}\n' "$id"
            ;;
        notifications/initialized) ;;
        tools/list)
            printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"%s","description":"d","inputSchema":{"type":"object"}}]}}\n' "$id" "$TEST_VAR_VALUE"
            ;;
    esac
done
"#;
        let mut env = HashMap::new();
        env.insert("TUIC_TEST_ENV_VAR".to_string(), "hello-from-env".to_string());

        let mut tmp = std::env::temp_dir();
        tmp.push(format!("tuic-mcp-env-test-{}.sh", uuid::Uuid::new_v4()));
        std::fs::write(&tmp, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let config = StdioConfig {
            name: "env-test".to_string(),
            command: "sh".to_string(),
            args: vec![tmp.to_str().unwrap().to_string()],
            env,
        };

        let mut client = StdioMcpClient::new(config);
        let tools = client.spawn_and_initialize().unwrap();
        assert_eq!(tools[0].original_name, "hello-from-env");
        client.shutdown();
    }

    #[test]
    fn from_upstream_config_returns_some_for_stdio() {
        let transport = crate::mcp_upstream_config::UpstreamTransport::Stdio {
            command: "npx".to_string(),
            args: vec![],
            env: HashMap::new(),
        };
        let client = StdioMcpClient::from_upstream_config("test".to_string(), &transport);
        assert!(client.is_some());
    }

    #[test]
    fn from_upstream_config_returns_none_for_http() {
        let transport = crate::mcp_upstream_config::UpstreamTransport::Http {
            url: "http://localhost:8080/mcp".to_string(),
        };
        let client = StdioMcpClient::from_upstream_config("test".to_string(), &transport);
        assert!(client.is_none());
    }
}
