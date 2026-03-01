# MCP Proxy Hub

TUICommander can act as a universal MCP (Model Context Protocol) proxy. Instead of configuring the same MCP servers in Claude Code, Cursor, and VS Code separately, you configure them once in TUICommander. All your AI clients connect to TUICommander's single `/mcp` endpoint and get access to every upstream tool automatically.

## How It Works

```
Claude Code ──┐
Cursor ───────┼──▶  TUICommander /mcp  ──┬──▶ GitHub MCP
VS Code ──────┘                           ├──▶ Filesystem MCP
                                          └──▶ Any MCP server
```

When a tool call arrives at TUICommander's MCP endpoint, it routes the request to the correct upstream server and returns the result. Upstream tools appear prefixed with the server name — for example, a tool called `search_code` from an upstream named `github` becomes `github__search_code`.

The MCP server must be enabled (Settings > Services > MCP Server).

## Adding an Upstream Server

Open Settings > Services > MCP Upstreams. Click **Add Server** and fill in:

### HTTP Server

Use this for MCP servers that expose a Streamable HTTP endpoint.

| Field | Example | Notes |
|-------|---------|-------|
| Name | `github` | Lowercase letters, digits, hyphens, underscores only |
| Type | HTTP | |
| URL | `https://mcp.example.com/mcp` | Must be `http://` or `https://` |
| Timeout | `30` | Seconds per request. 0 = no timeout |
| Enabled | On | Uncheck to disable without removing |

### Stdio Server

Use this for locally installed MCP servers (npm packages, Python scripts, etc.) that communicate over stdin/stdout.

| Field | Example | Notes |
|-------|---------|-------|
| Name | `filesystem` | Same naming rules as above |
| Type | Stdio | |
| Command | `npx` | Executable name or full path |
| Args | `-y @modelcontextprotocol/server-filesystem` | Space-separated |
| Env | `ALLOWED_PATHS=/home/user` | Optional extra environment variables |
| Enabled | On | |

Click **Save**. TUICommander connects immediately — no restart required.

## Server Names

The server name becomes the namespace prefix for all its tools. Choose names that are:
- Descriptive and short (`github`, `filesystem`, `db`)
- Lowercase only
- No spaces, dots, or capital letters — only `[a-z0-9_-]`
- Unique (no two servers can share a name)

## Authentication

For HTTP upstream servers that require a Bearer token:

1. Go to Settings > Services > MCP Upstreams
2. Find your server in the list
3. Click the key icon next to it
4. Enter your token

The token is stored in the OS keyring (Keychain on macOS, Credential Manager on Windows) — never in the config file.

To remove a credential, click the key icon and leave the field empty, then save.

## Tool Filtering

You can restrict which tools from an upstream are exposed to downstream clients. Edit a server and set the filter:

**Allow list** — only these tools are exposed:
```
Mode: allow
Patterns: read_*, list_*, get_*
```

**Deny list** — all tools except these are exposed:
```
Mode: deny
Patterns: delete_*, rm, drop_*, exec_*
```

Patterns support a trailing `*` for prefix matching. Exact names also work. There is no other wildcard syntax.

## Upstream Status

Each upstream server has a status indicator:

| Status | Meaning |
|--------|---------|
| Connecting | Handshake in progress |
| Ready | Connected, tools available |
| Circuit Open | Too many failures, retrying with backoff |
| Disabled | Disabled by you in config |
| Failed | Permanently failed — manual reconnect needed |

**Circuit breaker:** If an upstream fails 3 times consecutively, TUICommander stops sending requests to it briefly. Retries use exponential backoff starting at 1 second, capping at 60 seconds. After 10 retry cycles without recovery, the server is marked Failed.

To reconnect a Failed server, click **Reconnect** next to its name in the settings panel.

## Health Checks

TUICommander probes every `Ready` upstream every 60 seconds to verify it is still responding. If a probe fails, the circuit breaker activates. If a `Circuit Open` server's backoff has expired, the health check also attempts recovery.

## Hot-Reload

Adding, removing, or changing upstream servers takes effect immediately when you click Save. TUICommander computes a diff and only reconnects servers that actually changed — unchanged servers are never interrupted.

## Troubleshooting

### The upstream shows "Failed"

1. Check the server URL or command is correct.
2. Verify the server process is running (for stdio servers).
3. Check credentials are set if the server requires authentication.
4. Click **Reconnect** to retry.

### Tools are not appearing

- The upstream must be in `Ready` status for its tools to be included.
- Check that a tool filter is not hiding the tools you expect.
- Reconnect and check the error log (Cmd+Shift+E) for initialization errors.

### "Circular proxy" error

The HTTP URL you configured points to TUICommander's own MCP port. This would create an infinite loop. Use a different URL or port.

### "Invalid URL scheme" error

Only `http://` and `https://` URLs are accepted. Other schemes (ftp, file, javascript, etc.) are rejected for security.

### Stdio server crashes immediately

- Confirm the command exists on PATH (or use the full absolute path).
- Check the `Args` field for typos.
- Use the error log (Cmd+Shift+E) to see the stderr output from the child process.
- Note: the server cannot be respawned more than once every 5 seconds (rate limit).

### Credential not found

If the upstream returns 401 errors:
1. Go to Settings > Services > MCP Upstreams.
2. Click the key icon for the server.
3. Re-enter the Bearer token and save.

The credential lookup uses the server `name` as the keyring key. If you renamed the server, the old credential is no longer found — re-enter it under the new name.

## Example: Connecting the MCP Filesystem Server

Install the server:
```sh
npm install -g @modelcontextprotocol/server-filesystem
```

Add it in Settings > Services > MCP Upstreams:
- Name: `filesystem`
- Type: Stdio
- Command: `npx`
- Args: `-y @modelcontextprotocol/server-filesystem /path/to/allowed/dir`

After saving, the tool `filesystem__read_file` (and others) will appear in your AI client's tool list.

## Example: Connecting a Remote HTTP MCP Server

- Name: `github`
- Type: HTTP
- URL: `https://api.example.com/mcp`
- Timeout: 30

Set the Bearer token via the key icon in settings. Tools appear as `github__search_code`, `github__create_issue`, etc.

## Security Notes

- Config files (`mcp-upstreams.json`) never contain credentials — only the upstream name is stored. Tokens live in the OS keyring only.
- Stdio servers run with a sanitized environment. Your shell secrets (`ANTHROPIC_API_KEY`, `AWS_SECRET_ACCESS_KEY`, etc.) are not inherited by spawned MCP processes. Only `PATH`, `HOME`, `USER`, `LANG`, `LC_ALL`, `TMPDIR`, `TEMP`, `TMP`, `SHELL`, and `TERM` are passed through. Add anything else explicitly in the `Env` field.
- Self-referential HTTP URLs (pointing to TUIC's own MCP port) are rejected to prevent circular proxying.
- Only `http://` and `https://` URL schemes are accepted.
