# Lazygit Integration

TUICommander integrates with [lazygit](https://github.com/jesseduffield/lazygit), a terminal UI for git. Three modes let you use it however you prefer.

## Requirements

Lazygit must be installed on your system. TUICommander detects it via `resolve_cli()`, which probes well-known directories — it works even in release builds where the shell PATH isn't available.

If lazygit is not found, the shortcuts are disabled.

## Modes

### Inline Terminal (`Cmd+G`)

Opens lazygit in the active terminal tab, replacing your shell. When you quit lazygit (`q`), you're back in your shell.

Best for: Quick git operations without leaving your current context.

### Split Pane (`Cmd+Shift+L`)

Opens lazygit in a new split pane alongside your active terminal. Both the terminal and lazygit are visible at the same time.

- Navigate between panes with `Alt+←/→`
- Close the lazygit pane with `Cmd+W` to collapse back to a single pane

Best for: Running an agent in one pane while staging/committing in the other.

### Floating Window

Right-click a terminal tab → **Detach to Window**, then run lazygit in the detached window.

Best for: Keeping lazygit always visible on a second monitor.

## Tab Naming

When lazygit runs, TUICommander explicitly sets the tab name to "lazygit" to avoid polluted OSC title sequences that git operations can produce.

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Cmd+G` | Open lazygit in active terminal |
| `Cmd+Shift+L` | Open lazygit in a split pane |
| `Cmd+Shift+D` | Open the Git Panel (TUICommander's built-in git UI) |

## Git Panel vs Lazygit

TUICommander also has a built-in **Git Panel** (`Cmd+Shift+D`) for common git actions (staging, commit, push, pull, fetch, stash, blame, history). Use whichever fits your workflow:

- **Git Panel** — Docked side panel with Changes, Log, and Stashes tabs plus History/Blame sub-panels
- **Lazygit** — Full interactive git TUI with staging hunks, interactive rebase, etc.
