# TUICommander Relay Server

A lightweight, blind WebSocket relay that enables secure remote access to TUICommander from any network — no port forwarding, no VPN required.

## How It Works

The relay server acts as a **blind intermediary** between your computer running TUICommander and your phone's browser. It forwards encrypted blobs without ever being able to read their contents.

```
Your Computer                    Relay Server                     Your Phone
TUICommander  ──── WSS ────►  tuic-relay  ◄──── WSS ────  Mobile PWA
              encrypted                          encrypted
              blobs only                         blobs only
```

### End-to-End Encryption

**The relay server has zero knowledge of your data.** All communication between TUICommander and your phone is encrypted end-to-end with AES-256-GCM:

- The encryption key is generated on your computer and delivered to your phone via QR code
- The key is embedded in the URL **fragment** (`#`), which is [never sent to any server](https://www.rfc-editor.org/rfc/rfc3986#section-3.5) by the browser
- The relay sees only opaque binary blobs — it cannot decrypt terminal output, commands, file contents, or any other data
- Even if the relay server is compromised, your data remains encrypted and unreadable
- No encryption keys are ever stored or transmitted through the relay

### What the relay CAN see

Only connection metadata required for routing:

- Session ID (random UUID, not tied to your identity)
- Bearer token hash (for authentication)
- Timestamps and byte counts (for usage stats)
- Push notification hints: **type** of event ("question", "error") and **session name** — never the actual content

### What the relay CANNOT see

- Terminal output or commands
- File contents
- Agent conversations
- API keys, credentials, or any other secrets
- Anything encrypted with your E2E key

## Setup

### Option 1: Use the hosted relay (easiest)

TUICommander ships pre-configured to use `relay.tuicommander.com`. Just enable it in **Settings → Services → Remote Relay** and scan the QR code with your phone. No server setup needed.

### Option 2: Self-host with Docker Compose (recommended for self-hosting)

Clone the repo and run:

```bash
cd tools/relay
docker compose up -d
```

This starts the relay server with Caddy as a reverse proxy for automatic HTTPS (Let's Encrypt).

**Required configuration** — edit `docker-compose.yml` or set environment variables:

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `RELAY_BIND` | No | `0.0.0.0:8080` | Address the relay binds to |
| `RELAY_DB_PATH` | No | `./relay.db` | Path to SQLite database for stats |
| `RELAY_ADMIN_KEY` | Yes | — | Secret key for the `/stats/global` admin endpoint |
| `RELAY_VAPID_PRIVATE_KEY` | Yes | — | VAPID private key for Web Push notifications (base64) |
| `RELAY_VAPID_SUBJECT` | Yes | — | VAPID subject (e.g., `mailto:admin@example.com`) |
| `RELAY_MAX_SESSIONS_PER_TOKEN` | No | `5` | Max concurrent relay sessions per user |
| `RELAY_SESSION_TIMEOUT_SECS` | No | `3600` | Idle session timeout (seconds) |
| `RELAY_REGISTER_RATE_LIMIT` | No | `10/hour` | Registration rate limit per IP |

**Generate VAPID keys** (one-time):

```bash
# Using openssl
openssl ecparam -genkey -name prime256v1 -out vapid_private.pem
openssl ec -in vapid_private.pem -pubout -out vapid_public.pem

# Or use the relay's built-in command:
docker run --rm tuic-relay generate-vapid
```

Then point TUICommander to your relay: **Settings → Services → Remote Relay → Relay URL** = `wss://your-domain.com`.

### Option 3: Build from source

```bash
cd tools/relay
cargo build --release
./target/release/tuic-relay --bind 0.0.0.0:8080
```

Place a reverse proxy (Caddy, nginx, Traefik) in front for TLS termination.

## Architecture

```
tools/relay/
├── Cargo.toml
├── Dockerfile              # Multi-stage: compiles inside Docker
├── docker-compose.yml      # Self-hosting with Caddy auto-HTTPS
├── src/
│   ├── main.rs             # CLI entry point
│   ├── lib.rs              # Library root
│   ├── routes.rs           # Axum HTTP/WS routes
│   ├── relay.rs            # Session pairing + message forwarding
│   ├── auth.rs             # Bearer token auth (argon2id)
│   ├── db.rs               # SQLite stats (tokio-rusqlite)
│   ├── push.rs             # Web Push notifications (VAPID)
│   └── types.rs            # Shared types
└── tests/
    └── relay_core.rs       # Integration tests
```

### API Endpoints

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| `GET` | `/health` | None | Health check |
| `POST` | `/register` | Rate-limited | Self-registration, returns bearer token |
| `WS` | `/ws/{sessionId}` | Bearer (first message) | WebSocket relay — max 2 peers per session |
| `GET` | `/stats` | Bearer | Per-user session statistics |
| `GET` | `/stats/global` | Admin key | Global statistics |
| `POST` | `/push/subscribe` | Bearer | Register push notification subscription |
| `DELETE` | `/push/subscribe` | Bearer | Unregister push subscription |

### Resource Usage

The relay is extremely lightweight:

| Metric | Value |
|--------|-------|
| Docker image | ~10 MB |
| RAM per 100 concurrent sessions | ~50 MB |
| CPU | Negligible (pure I/O forwarding) |
| Disk | SQLite file, a few MB even with thousands of sessions |

A $5/month VPS comfortably handles hundreds of concurrent users.

## Relationship to Direct Access

The relay **does not replace** TUICommander's existing direct HTTP access. If you're on the same network (LAN, Tailscale, VPN), the direct connection at `http://{ip}:9876/mobile` continues to work with no encryption overhead. The relay is an additional option for when you need access from any network without VPN setup.

## Security Model

- **E2E encryption**: AES-256-GCM, key never touches the relay
- **Token hashing**: argon2id (GPU-resistant)
- **Rate limiting**: Per-IP registration, per-token message rate
- **Session limits**: Max 2 peers per session, max 5 sessions per token
- **Timeouts**: Idle sessions cleaned up after 1 hour (configurable)
- **TLS**: Terminated at reverse proxy (Caddy auto-HTTPS recommended)
- **Zero persistence of user data**: Only connection metadata stored in SQLite

## License

MIT — same as TUICommander.
