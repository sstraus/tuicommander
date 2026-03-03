# Agent Teams

Agent Teams is a Claude Code feature that spawns teammate agents in separate terminal panes. TUICommander provides an `it2` shim that intercepts these iTerm2 CLI calls and routes them through TUIC tabs instead of tmux panes.

## Setup

1. Open **Settings > General > Agent Teams**
2. Toggle **Enable it2 shim** on
3. Restart TUICommander (or the shim installs immediately)

The shim script is installed at `~/.tuicommander/bin/it2`.

## How It Works

When Claude Code detects `TERM_PROGRAM=iTerm.app` and finds `it2` on `PATH`, it uses the iTerm2 session CLI to create teammate terminals. TUICommander intercepts these calls:

| Claude Code Action | it2 Command | TUIC Behavior |
|---|---|---|
| Create teammate tab | `it2 session split -v` | Creates a new TUIC terminal via HTTP API |
| Send command to teammate | `it2 session run -s <id> <cmd>` | Writes command to the PTY session |
| Close teammate | `it2 session close -s <id>` | Destroys the PTY session |
| List teammates | `it2 session list` | Lists all active sessions |

## Environment Variables

When Agent Teams is enabled, each new terminal receives these environment variables:

| Variable | Value | Purpose |
|---|---|---|
| `ITERM_SESSION_ID` | `w0t0p0:<session_id>` | iTerm2 session identifier format |
| `TERM_PROGRAM` | `iTerm.app` | Triggers Claude Code's iTerm2 detection |
| `PATH` | `~/.tuicommander/bin:$PATH` | Ensures `it2` shim is found first |
| `TUIC_HTTP_PORT` | Port number | HTTP API port for the shim |
| `TUIC_SOCKET_PATH` | Socket path | Unix socket for local API communication |

## Communication

The shim communicates with TUIC's HTTP API via Unix domain socket (`TUIC_SOCKET_PATH`). This works without the TCP remote access server being enabled.

## Requirements

- Claude Code with Agent Teams support
- macOS or Linux (the shim is a bash script)
- TUICommander HTTP server running (always active)

## Troubleshooting

**Agent Teams not creating tabs:**
- Verify the shim exists: `ls -la ~/.tuicommander/bin/it2`
- Check env vars in terminal: `echo $TERM_PROGRAM` should show `iTerm.app`
- Test shim directly: `it2 --version` should print `it2 (TUICommander shim) 1.0.0`

**Shim errors about TUIC_SOCKET_PATH:**
- The socket is only available in terminals created by TUIC
- Running `it2` from an external terminal will fail with this error
