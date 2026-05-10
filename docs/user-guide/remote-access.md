# Remote Access

Access TUICommander from a browser on another device on your network.

## Setup

1. Open **Settings** (`Cmd+,`) → **Services** → **Remote Access**
2. Configure:
   - **Port** — Default `9876` (range 1024–65535)
   - **Username** — Basic Auth username
   - **Password** — Basic Auth password (stored as a bcrypt hash, never in plaintext)
3. Enable remote access

Once enabled, the settings panel shows the access URL: `http://<your-ip>:<port>`

## Connecting from Another Device

1. Open a browser on any device on the same network
2. Navigate to the URL shown in settings (e.g., `http://192.168.1.42:9876`)
3. Enter the username and password you configured
4. TUICommander loads in the browser with full terminal access

### QR Code

The settings panel shows a QR code for the access URL — scan it from a phone or tablet to connect quickly. The QR code uses your actual local IP address.

## What Works Remotely

The browser client provides the same UI as the desktop app:

- Terminal sessions (via WebSocket streaming)
- Sidebar with repositories and branches
- Diff, Markdown, and File Browser panels
- Keyboard shortcuts

## Security

- **Authentication** — Basic Auth with bcrypt-hashed passwords
- **Local network only** — The server binds to your machine's IP; it's not exposed to the internet unless you configure port forwarding (don't do this without a VPN)
- **CORS** — When remote access is enabled, any origin is allowed (necessary for browser access from different IPs)

## MCP HTTP Server

Separate from remote access, TUICommander runs an **HTTP API server** for AI tool integration:

- The server always listens on an IPC listener: Unix domain socket at `<config_dir>/mcp.sock` on macOS/Linux, or named pipe `\\.\pipe\tuicommander-mcp` on Windows
- AI agents connect via the `tuic-bridge` sidecar binary, which translates MCP stdio transport to the IPC listener
- Bridge configs are auto-installed on first launch for supported agents (Claude Code, Cursor, Windsurf, VS Code, Zed, Amp, Gemini). On every subsequent launch, the bridge path is verified and updated if stale (from reinstalls, updates, or moves)
- The `mcp_server_enabled` toggle in **Settings** → **Services** controls whether MCP protocol tools are exposed, not the server itself
- Shows server status and active session count in settings

The Unix socket is accessible only to the current user (filesystem permissions) and requires no authentication — it's designed for local tool integration, not remote access.

## Mobile Companion

TUICommander includes a phone-optimized interface for monitoring agents from your phone.

### Accessing the Mobile UI

1. Enable remote access (see Setup above)
2. Navigate to `http://<your-ip>:<port>/mobile` from your phone
3. Log in with your credentials

### Add to Home Screen

The mobile UI supports PWA (Progressive Web App) installation:

- **iOS Safari**: Tap Share → "Add to Home Screen"
- **Android Chrome**: Tap the three-dot menu → "Add to Home screen"

The app launches in standalone mode (no browser chrome) for a native-like experience.

### Mobile Features

- **Sessions list** — See all running agents with status (idle, busy, question, rate-limited, error)
- **Session detail** — Live output streaming, quick-reply chips (Yes/No/Enter/Ctrl-C), text input
- **Question banner** — Instant notification when any agent needs input, with quick-reply buttons
- **Activity feed** — Chronological event feed grouped by time
- **Notification sounds** — Audio alerts for questions, errors, completions, and rate limits

### Tips

- Pull down on the sessions list to refresh
- The question banner appears on all screens — you don't need to be on the sessions tab to respond
- Sound notifications can be toggled in the mobile Settings tab

## SSH Tunnel Management

TUICommander can manage persistent SSH tunnels with automatic reconnection, port forwarding, and audit logging.

### Creating a Tunnel Profile

1. Open **Settings** (`Cmd+,`) → **Services** → **SSH Tunnels**
2. Click **Add Tunnel** to open the editor
3. Configure:
   - **Name** — A descriptive label (e.g., "prod-db-tunnel")
   - **Host** — Remote SSH host
   - **Port** — SSH port (default 22)
   - **User** — SSH username
   - **Identity File** — Optional path to SSH private key (use the Browse button to select)
   - **Port Forwards** — Local or remote port forwarding rules (e.g., local 8080 → remote 80). The remote host is pre-populated from the tunnel host when adding a forward
   - **Options** — ServerAliveInterval (default 15s), ServerAliveCountMax (default 3), StrictHostKeyChecking (Yes or AcceptNew)
4. Save the profile

Tunnel profiles are stored as TOML files. **Global profiles** live in `<config_dir>/tunnels/` and are available across all repos. **Per-repo profiles** are stored in `<repo>/.tuic/tunnels/` and override global profiles with the same ID.

### Auto-Connect

Enable **Auto-Connect** on a tunnel profile to have it start automatically when TUICommander launches. Useful for tunnels you always need (database access, internal services).

Toggle auto-connect in the tunnel editor — profiles marked with auto-connect are started during app hydration before you interact with the UI.

### Statusbar Indicator

The status bar shows a shield icon for SSH tunnels:

- **Grey shield** — You have tunnel profiles configured but none are currently connected
- **Green shield with badge** — Shows the number of active tunnel connections

Click the shield to open the Tunnels Panel.

### Command Palette

Open the command palette (`Cmd+P` / `Ctrl+P`) and type "tunnels" to toggle the Tunnels Panel without navigating to Settings.

### Starting and Stopping Tunnels

- In the **Tunnels Panel**, click the **Start** button next to a profile to launch the SSH tunnel
- The **TunnelStatusBadge** shows the current state: Starting, Connected, Reconnecting, Stopped, or Error
- Click **Stop** to gracefully terminate the SSH process (SIGTERM with 5s grace period, then SIGKILL)
- On app exit, all active tunnels are automatically stopped — no orphaned SSH processes

### SSH Agent Detection

TUICommander automatically detects your SSH agent and shows the agent type and loaded keys in the tunnel editor. Supported agents:

- **1Password** — Detected via the 1Password SSH agent socket
- **Secretive** — Detected via the Secretive agent socket
- **GPG Agent** — Detected via gpg-agent socket
- **Generic SSH Agent** — Any other `SSH_AUTH_SOCK` value

The key listing shows fingerprint, comment, and key type for each loaded key, helping you verify that the correct identity is available before connecting.

### Automatic Reconnection

When a tunnel disconnects due to a network issue or timeout, the supervisor automatically reconnects with exponential backoff:

- Base delay: 1 second, doubling each attempt
- Maximum delay: 30 seconds
- Jitter: +/-25% to prevent thundering herd
- Maximum retries: 10 before giving up
- Backoff resets on successful connection

Non-retryable failures (authentication errors, host key mismatches) stop immediately without retry.

### Audit Log

All tunnel events (start, connect, disconnect, error, retry, stop) are recorded in a SQLite database with WAL mode for performance. The audit log supports:

- Querying events by tunnel ID
- Querying events by time range
- Automatic rotation of old events (configurable retention period)

### Exit Classification

The supervisor classifies SSH process exits to determine whether retry is appropriate:

| Exit Reason | Retryable | Description |
|-------------|-----------|-------------|
| AuthFailed | No | Permission denied or authentication failure |
| HostKeyMismatch | No | Remote host key changed |
| PortInUse | No | Local forwarding port already bound |
| ConnectionRefused | Yes | Remote host rejected the connection |
| NetworkDown | Yes | Network unreachable |
| Timeout | Yes | Connection timed out |
| UserKilled | No | Process terminated by user signal |

## Remote Connection Manager

Remote connections let you manage `tuic-remote` daemons running on other machines. TUICommander routes API calls to the correct host based on which repo/session is active.

### Adding an SSH Connection

1. Open **Settings** → **Connections** → **Add Connection**
2. Select **SSH** transport
3. Configure host, port (default 22), user, and optional identity file
4. Set the remote daemon port (default 9877)
5. Save — an SSH tunnel is automatically created to forward the daemon port

### Adding a Direct Connection

1. Open **Settings** → **Connections** → **Add Connection**
2. Select **Direct** transport
3. Enter the URL of the remote daemon (e.g., `http://10.0.0.5:9877`)
4. Set the auth username
5. Save — health polling begins immediately

### Remote Repositories and Terminals

Once a remote connection is configured:

- **Add remote repo** — When adding a repository, select a connection. The repo appears in the sidebar with a remote badge
- **Open terminal** — Terminals on remote repos connect via WebSocket to the remote daemon. I/O works identically to local terminals
- **Health monitoring** — Connection health is polled periodically. Disconnected connections show a warning badge in the sidebar

Connections are stored in `<config_dir>/connections.json` with SSH and Direct transport types.

## tuic-remote (Beta)

A standalone headless daemon for running TUICommander on a Linux server without a desktop environment. It exposes the same HTTP/WebSocket API as the desktop app's remote access feature, but runs as an independent binary — no Tauri, no GUI.

### Installation

Download the `tuic-remote` binary for your platform from the [GitHub Releases](https://github.com/sstraus/tuicommander/releases) page.

| Platform | Artifact |
|----------|----------|
| Linux x64 | `tuic-remote-x86_64-unknown-linux-gnu` |
| Linux ARM64 | `tuic-remote-aarch64-unknown-linux-gnu` |
| macOS ARM (Apple Silicon) | `tuic-remote-aarch64-apple-darwin` |
| Windows x64 | `tuic-remote-x86_64-pc-windows-msvc.exe` |

```bash
# Example: Linux x64
curl -fsSL -o tuic-remote https://github.com/sstraus/tuicommander/releases/latest/download/tuic-remote-x86_64-unknown-linux-gnu
chmod +x tuic-remote
```

### Setup

Set a password before first use:

```bash
./tuic-remote --set-password
```

This stores a bcrypt hash in the TUICommander config directory (`~/.config/tuicommander/` on Linux).

### Running

```bash
# Default port 9877
./tuic-remote

# Custom port
TUIC_PORT=8080 ./tuic-remote
```

The daemon binds to `0.0.0.0:<port>` and serves:
- The TUICommander web UI (PWA-capable)
- WebSocket terminal streaming
- MCP tool integration (for AI agents)

### TLS

Configure TLS via the TUICommander config file (`~/.config/tuicommander/config.toml`):

```toml
[services.tls]
cert_path = "/path/to/cert.pem"
key_path = "/path/to/key.pem"
```

### Differences from Desktop Remote Access

| | Desktop Remote Access | tuic-remote |
|---|---|---|
| Requires desktop app | Yes | No |
| Runs headless | No | Yes |
| Tauri dependency | Yes | No |
| Default port | 9876 | 9877 |
| LAN auth bypass | Configurable | Always disabled |
| Signal handling | N/A | Graceful SIGINT/SIGTERM |

### Status

**Beta** — the core HTTP/WebSocket API is stable, but the standalone daemon is new and may have rough edges. Report issues on GitHub.

## Troubleshooting

| Problem | Fix |
|---------|-----|
| Can't connect from another device | Check that both devices are on the same network. Try pinging the host IP. |
| Connection refused | Verify the port isn't blocked by a firewall. The settings panel includes a reachability check. |
| Authentication fails | Re-enter the password in settings — the stored bcrypt hash may be from a different password. |
| Terminals not responding | WebSocket connection may have dropped. Refresh the browser page. |
