//! A deterministic MCP server over stdio, for the `mcp_proxy::stdio_client` tests.
//!
//! Those tests used to write a `#!/bin/sh` script to a temp file and run
//! `sh <script>`, with `sed` pulling the method and the id out of each request
//! line. `sh` and `sed` are not Windows programs: the nine tests that did this
//! passed on GitHub's `windows-latest` only because that image carries
//! `C:\Program Files\Git\usr\bin` on `PATH`. On a Windows machine without it
//! every one of them failed to spawn, so the Windows job proved nothing about
//! the stdio client. Measured 2026-09-16 on a real Windows 11 box: 9 failures,
//! all of them the spawn.
//!
//! One binary replaces the eight scripts. Each scenario is named on the command
//! line and is the same dispatch the script was — read a request line, answer
//! by method — written once, in the language the rest of the suite is in.
//!
//! It is a test fixture, not a product binary: Tauri bundling lists production
//! binaries and sidecars explicitly, so this target never ships.

use std::io::{BufRead, Write};

use serde_json::{Value, json};

fn main() {
    let scenario = std::env::args()
        .nth(1)
        .expect("usage: tuic-mcp-fixture-server <scenario>");

    // Read once, as the shell scripts did: the value a scenario reports is the
    // one the parent set when it spawned this process.
    let env_var = std::env::var("TUIC_TEST_ENV_VAR").unwrap_or_default();

    // `protocol-offer-probe` answers a question about a *past* request, so it
    // needs one bit of memory across the loop.
    let mut offered = "wrong";

    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line.expect("read a request line");
        let request: Value = serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("{scenario}: not a JSON-RPC frame: {line:?} ({e})"));
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let id = request.get("id").cloned().unwrap_or(Value::Null);

        match (scenario.as_str(), method) {
            // ---------------------------------------------------------------
            ("minimal", "initialize") => {
                reply(&id, initialize_result("2025-03-26", json!({"tools": {}})));
            }
            ("minimal", "notifications/initialized") => {}
            ("minimal", "tools/list") => {
                reply(&id, json!({"tools": [tool("echo", "Echo tool")]}));
            }
            ("minimal", "tools/call") => reply(&id, echoed("echoed")),
            // The only scenario with a default arm. The others answer the
            // methods their test drives and stay silent otherwise, which is
            // what their `case` did.
            ("minimal", _) => {
                emit(&json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {"code": -32601, "message": "Method not found"},
                }));
            }

            ("delayed-tool-call", "initialize") => {
                reply(&id, initialize_result("2025-11-25", json!({"tools": {}})));
            }
            ("delayed-tool-call", "tools/list") => {
                reply(&id, json!({"tools": [tool("echo", "Delayed echo")]}));
            }
            ("delayed-tool-call", "tools/call") => {
                let args = &request["params"]["arguments"];
                std::thread::sleep(std::time::Duration::from_millis(
                    args["delay_ms"].as_u64().unwrap(),
                ));
                reply(&id, echoed(args["message"].as_str().unwrap()));
            }
            ("delayed-tool-call", _) => {}

            // A real crash/recovery probe. Each process reports its own pid.
            ("crash-on-call", "initialize") => {
                reply(&id, initialize_result("2025-11-25", json!({"tools": {}})));
            }
            ("crash-on-call", "tools/list") => {
                reply(&id, json!({"tools": [tool("probe", "Process identity")]}));
            }
            ("crash-on-call", "tools/call") => {
                if request["params"]["arguments"]["crash"] == true {
                    std::process::exit(0);
                }
                reply(&id, echoed(&std::process::id().to_string()));
            }
            ("crash-on-call", _) => {}

            // ---------------------------------------------------------------
            ("protocol-offer-probe", "initialize") => {
                if line.contains("\"protocolVersion\":\"2025-11-25\"") {
                    offered = "legacy";
                }
                reply(&id, initialize_result("2025-11-25", json!({"tools": {}})));
            }
            ("protocol-offer-probe", "tools/list") => {
                reply(
                    &id,
                    json!({"tools": [tool(offered, "Protocol offer probe")]}),
                );
            }

            // ---------------------------------------------------------------
            ("exit-after-handshake", "initialize") => {
                reply(&id, initialize_result("2025-03-26", json!({})));
            }
            ("exit-after-handshake", "tools/list") => {
                reply(&id, json!({"tools": []}));
                std::process::exit(0);
            }

            // ---------------------------------------------------------------
            ("env-tool-name", "initialize") => {
                reply(&id, initialize_result("2025-03-26", json!({})));
            }
            ("env-tool-name", "tools/list") => {
                reply(&id, json!({"tools": [tool(&env_var, "d")]}));
            }

            // ---------------------------------------------------------------
            ("notify-once", "initialize") => {
                reply(&id, initialize_result("2025-03-26", json!({"tools": {}})));
            }
            ("notify-once", "notifications/initialized") => {
                // A server notification, which has no id. This is what used to
                // make the client report "0 tools".
                emit(&json!({"jsonrpc": "2.0", "method": "notifications/tools/list_changed"}));
            }
            ("notify-once", "tools/list") => {
                reply(
                    &id,
                    json!({"tools": [tool("alpha", "A"), tool("beta", "B")]}),
                );
            }
            ("notify-once", "tools/call") => reply(&id, echoed("ok")),

            // ---------------------------------------------------------------
            ("notify-thrice", "initialize") => {
                reply(&id, initialize_result("2025-03-26", json!({"tools": {}})));
            }
            ("notify-thrice", "notifications/initialized") => {
                emit(&json!({"jsonrpc": "2.0", "method": "notifications/tools/list_changed"}));
                emit(&json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/progress",
                    "params": {"token": "abc"},
                }));
                emit(&json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/message",
                    "params": {"level": "info", "data": "starting"},
                }));
            }
            ("notify-thrice", "tools/list") => {
                reply(&id, json!({"tools": [tool("gamma", "G")]}));
            }
            ("notify-thrice", "tools/call") => reply(&id, echoed("ok")),

            // ---------------------------------------------------------------
            ("chatty", "initialize") => {
                reply(&id, initialize_result("2025-03-26", json!({"tools": {}})));
            }
            ("chatty", "notifications/initialized") => {
                for n in 0..500 {
                    emit(&json!({
                        "jsonrpc": "2.0",
                        "method": "notifications/message",
                        "params": {"level": "info", "n": n},
                    }));
                }
            }
            ("chatty", "tools/list") => {
                reply(&id, json!({"tools": [tool("delta", "D")]}));
            }

            // ---------------------------------------------------------------
            ("mute-tool-call", "initialize") => {
                reply(&id, initialize_result("2025-03-26", json!({"tools": {}})));
            }
            ("mute-tool-call", "tools/list") => {
                reply(&id, json!({"tools": [tool("hang", "H")]}));
            }
            // Silence. The upstream is alive and will never answer.
            ("mute-tool-call", "tools/call") => {}

            // ---------------------------------------------------------------
            // A method the scenario has nothing to say about. The shell scripts
            // ignored these; only `minimal` had a default arm, and it is above.
            (
                "protocol-offer-probe"
                | "exit-after-handshake"
                | "env-tool-name"
                | "notify-once"
                | "notify-thrice"
                | "chatty"
                | "mute-tool-call",
                _,
            ) => {}
            (unknown, _) => panic!("unknown scenario {unknown:?}"),
        }
    }
}

/// The `initialize` result every scenario returns, minus the two fields they
/// disagree about.
fn initialize_result(protocol: &str, capabilities: Value) -> Value {
    json!({
        "protocolVersion": protocol,
        "capabilities": capabilities,
        "serverInfo": {"name": "test", "version": "1.0"},
    })
}

/// One tool, with the empty object schema these tests never look at.
fn tool(name: &str, description: &str) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {"type": "object"},
    })
}

/// The `tools/call` result shape.
fn echoed(text: &str) -> Value {
    json!({"content": [{"type": "text", "text": text}], "isError": false})
}

/// Answer a request that carried an id.
fn reply(id: &Value, result: Value) {
    emit(&json!({"jsonrpc": "2.0", "id": id, "result": result}));
}

/// One frame, one line, flushed. The client reads line by line and the parent
/// is blocked waiting, so a frame left in a buffer is a hang.
fn emit(frame: &Value) {
    let mut stdout = std::io::stdout();
    writeln!(stdout, "{frame}").expect("write frame");
    stdout.flush().expect("flush frame");
}
