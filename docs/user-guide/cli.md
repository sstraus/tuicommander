# tuic CLI

The `tuic` command line tool lets you control TUICommander from the terminal. It combines the best of VS Code's `code` CLI, Zed's editor integration, and tmux's session management into a single binary.

## Installation

**From the app:** Settings > General > Command Line Interface > Install tuic CLI

**First launch:** TUICommander offers to install the CLI on first run.

**From the CLI itself:** `tuic install-cli`

The binary is installed to:
- **macOS:** `/usr/local/bin/tuic` (requires admin password)
- **Linux:** `/usr/local/bin/tuic` (requires sudo)
- **Windows:** `%LOCALAPPDATA%\Microsoft\WindowsApps\tuic.exe` (no admin needed)

The CLI auto-updates silently when TUICommander starts — no manual update needed.

## Opening Files and Repos

```bash
# Open a file (launches TUICommander if not running)
tuic file.rs

# Open at specific line and column
tuic file.rs:42
tuic file.rs:42:10
tuic open --goto file.rs:42

# Open the current directory as a repo
tuic .
tuic /path/to/project

# Open with --wait (for use as $EDITOR)
tuic open --wait file.rs

# Diff two files
tuic diff old.rs new.rs
```

### Using as $EDITOR

```bash
export EDITOR="tuic open --wait"
git commit  # opens commit message in TUICommander
```

## Session Management

These commands mirror tmux semantics:

```bash
# List all sessions
tuic ls

# Create a new session
tuic new
tuic new -n "my-session"
tuic new -n "build" /path/to/repo

# Send input to a session
tuic send <id-or-name> "make test" Enter

# Capture session output
tuic capture <id-or-name>
tuic capture <id-or-name> --format raw

# Kill a session
tuic kill <id-or-name>

# Resize a session
tuic resize <id-or-name> 120x40

# Pause/resume output
tuic pause <id-or-name>
tuic resume <id-or-name>
```

Session targets accept full UUIDs, ID prefixes, or session names.

## Agent Orchestration

```bash
# Spawn an AI agent
tuic agent spawn claude
tuic agent spawn codex /path/to/repo

# List running agents
tuic agent ls

# Send a message to an agent
tuic agent send <id> "fix the tests"
```

## tmux Compatibility

`tuic` can act as a drop-in replacement for tmux. When invoked as `tmux` (via symlink), it translates tmux commands to TUICommander equivalents.

### Setting Up the Alias

```bash
# Create tmux -> tuic symlink
tuic alias

# Remove the alias (restores original tmux if installed)
tuic alias --remove
```

### Supported tmux Commands

When invoked as `tmux`, the following commands are supported:

| tmux Command | Behavior |
|---|---|
| `tmux` | Create new session in cwd |
| `tmux new-session -s name` | Create named session |
| `tmux list-sessions` | List sessions |
| `tmux kill-session -t target` | Kill session |
| `tmux kill-server` | Kill all sessions |
| `tmux send-keys -t target "cmd" Enter` | Send input |
| `tmux capture-pane -t target` | Capture output |
| `tmux resize-pane -t target -x 120 -y 40` | Resize |
| `tmux attach-session` | Focus TUICommander window |
| `tmux has-session -t target` | Check if session exists (exit code) |

Key names are translated: `Enter`, `Space`, `Tab`, `Escape`, `C-c`, `C-d`, `C-z`, etc.

## System Commands

```bash
# Check TUICommander status
tuic status

# Install CLI to system PATH
tuic install-cli
tuic install-cli --path /custom/path

# Create/remove tmux alias
tuic alias
tuic alias --remove
```

## IPC Architecture

The CLI communicates with TUICommander via IPC:
- **macOS/Linux:** Unix domain socket at `~/.config/com.tuic.commander/mcp.sock`
- **Windows:** Named pipe at `\\.\pipe\tuicommander-mcp`

Override with `$TUIC_SOCKET` environment variable.

If TUICommander is not running, `tuic open` and `tuic new` will launch it automatically.
