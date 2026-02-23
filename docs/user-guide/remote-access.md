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

Separate from remote access, TUICommander also runs an **MCP HTTP server** for AI tool integration:

- Enable in **Settings** → **Services** → **MCP HTTP Server**
- Exposes REST, WebSocket, and SSE endpoints on localhost
- Used by Claude Code, Cursor, and other AI tools via the MCP protocol
- Shows server status, port, and active session count in settings

The MCP server is localhost-only and doesn't require authentication — it's designed for local tool integration, not remote access.

## Troubleshooting

| Problem | Fix |
|---------|-----|
| Can't connect from another device | Check that both devices are on the same network. Try pinging the host IP. |
| Connection refused | Verify the port isn't blocked by a firewall. The settings panel includes a reachability check. |
| Authentication fails | Re-enter the password in settings — the stored bcrypt hash may be from a different password. |
| Terminals not responding | WebSocket connection may have dropped. Refresh the browser page. |
