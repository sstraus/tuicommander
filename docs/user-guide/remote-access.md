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

- The server always listens on a Unix domain socket at `<config_dir>/mcp.sock` — no port configuration needed
- AI agents connect via the `tuic-mcp-bridge` sidecar binary, which translates MCP stdio transport to HTTP over the Unix socket
- Bridge configs are auto-installed on first launch for supported agents (Claude Code, Cursor, Windsurf, VS Code, Zed, Amp, Gemini)
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

## Troubleshooting

| Problem | Fix |
|---------|-----|
| Can't connect from another device | Check that both devices are on the same network. Try pinging the host IP. |
| Connection refused | Verify the port isn't blocked by a firewall. The settings panel includes a reachability check. |
| Authentication fails | Re-enter the password in settings — the stored bcrypt hash may be from a different password. |
| Terminals not responding | WebSocket connection may have dropped. Refresh the browser page. |
