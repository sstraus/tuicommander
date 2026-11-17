"""Record client tool names without vendor calls, real credentials or model execution.

A local MCP probe advertises one TUIC-named tool; the local API sink records
only tool names from each installed client's outgoing model request, then rejects
it. Isolated client homes and dummy keys keep user token stores out of scope.
"""
import http.server
import json
import os
from pathlib import Path
import subprocess
import sys
import threading

root = Path(__file__).resolve().parents[4]
work = Path(sys.argv[1]).resolve()
# Scratch work dir under the host temp dir, never inside the real HOME.
assert Path(os.environ.get("TUIC_TEST_HOST_TMPDIR") or tempfile.gettempdir()).resolve() in work.parents
work.mkdir(exist_ok=False)
records = {}


class Probe(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def reply(self, value, status=200):
        body = json.dumps(value).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        self.reply({}, 405)

    def do_POST(self):
        value = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))))
        if self.path == "/mcp":
            method = value.get("method")
            if "id" not in value:
                self.send_response(202)
                self.end_headers()
                return
            if method in ("initialize", "server/discover"):
                result = {"protocolVersion": value.get("params", {}).get("protocolVersion", "2025-11-25"), "capabilities": {"tools": {}}, "serverInfo": {"name": "tuicommander", "version": "probe-2108"}}
            elif method == "tools/list":
                result = {"tools": [{"name": "agent", "description": "TUIC managed peers and mail probe", "inputSchema": {"type": "object", "properties": {"action": {"type": "string"}}, "required": ["action"]}}]}
            else:
                result = {}
            self.reply({"jsonrpc": "2.0", "id": value["id"], "result": result})
            return
        names = []
        for tool in value.get("tools", []):
            name = tool.get("name") or tool.get("function", {}).get("name")
            if name:
                names.append(name)
                for child in tool.get("tools", []):
                    if child.get("name"):
                        names.append(name + "." + child["name"])
        if names:
            records[active_client] = sorted(set(names))
        self.reply({"error": {"type": "invalid_request_error", "message": "Local capture complete; no model execution"}}, 400)


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Probe)
threading.Thread(target=server.serve_forever, daemon=True).start()
base = f"http://127.0.0.1:{server.server_port}"
results = {}
try:
    for active_client in ["claude", "codex"]:
        home = work / active_client
        home.mkdir()
        env = dict(os.environ, HOME=str(home), TMPDIR=str(work), ANTHROPIC_API_KEY="local-probe-not-a-secret", OPENAI_API_KEY="local-probe-not-a-secret", ANTHROPIC_BASE_URL=base, CLAUDE_CONFIG_DIR=str(home), CODEX_HOME=str(home), CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC="1", DISABLE_AUTOUPDATER="1", DISABLE_TELEMETRY="1")
        for name in ["CLAUDECODE", "TUIC_SESSION", "TUIC_PARENT", "CLAUDE_CODE_OAUTH_TOKEN"]:
            env.pop(name, None)
        if active_client == "claude":
            cmd = ["claude", "--bare", "--print", "--no-session-persistence", "--strict-mcp-config", "--mcp-config", json.dumps({"mcpServers": {"tuicommander": {"type": "http", "url": base + "/mcp"}}}), "--system-prompt", "Tool naming capture only", "--model", "claude-sonnet-4-6", "--output-format", "json", "List available TUIC tool names without calling tools"]
        else:
            config = f'''model = "gpt-5.4"
model_provider = "local_capture"
cli_auth_credentials_store = "file"
mcp_oauth_credentials_store = "file"
[model_providers.local_capture]
name = "Local capture"
base_url = "{base}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
[mcp_servers.tuicommander]
url = "{base}/mcp"
'''
            (home / "config.toml").write_text(config)
            cmd = ["codex", "--no-daemon", "exec", "--skip-git-repo-check", "List available TUIC tool names without calling tools"]
        try:
            run = subprocess.run(cmd, cwd=home, env=env, capture_output=True, text=True, timeout=40)
            results[active_client] = {"exit": run.returncode}
            (work / (active_client + ".log")).write_text(run.stdout + run.stderr)
        except subprocess.TimeoutExpired as error:
            results[active_client] = {"timeout": True}
            (work / (active_client + ".log")).write_bytes((error.stdout or b"") + (error.stderr or b""))
        results[active_client]["version"] = subprocess.check_output([active_client, "--version"], text=True).strip()
        results[active_client]["exposed_tool_names"] = records.get(active_client, [])
    output = {"source": "Installed clients' outgoing tool schema captured by a localhost-only API sink; MCP server alias tuicommander advertises agent. Dummy keys; no model execution.", "clients": results}
    assert "mcp__tuicommander__agent" in results["claude"]["exposed_tool_names"], results["claude"]
    assert "mcp__tuicommander.agent" in results["codex"]["exposed_tool_names"], results["codex"]
    target = root / "src-tauri/src/mcp_http/fixtures/tool-names-2108.json"
    target.write_text(json.dumps(output, indent=2) + "\n")
    print(target)
    print(json.dumps(output, indent=2))
finally:
    server.shutdown()
