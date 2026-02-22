# Keyboard Shortcuts Comparison Across Tools

Research date: 2026-02-22

## Legend

- **--** = not bound / not used by default
- **?** = could not verify from documentation (tool not installed or docs incomplete)
- Cursor inherits VS Code shortcuts for non-AI features (it's a VS Code fork)

---

## TUI Commander Current Shortcuts (for reference)

| Shortcut | TUI Commander |
|----------|---------------|
| Cmd+Shift+D | Toggle git diff panel |
| Cmd+E | Toggle file browser |
| Cmd+G | Open lazygit |
| Cmd+J | Toggle task queue |
| Cmd+K | Prompt library |
| Cmd+M | Toggle markdown panel |
| Cmd+N | Toggle ideas panel |
| Cmd+R | Run saved command |
| Cmd+\ | Split vertically |
| Cmd+Shift+G | Git operations panel |
| Cmd+Shift+L | Lazygit split pane |
| Cmd+Shift+T | Reopen closed tab |
| Cmd+Shift+R | Edit saved command |
| Alt+arrows | Navigate panes |

---

## Main Comparison: Cmd+Letter Shortcuts

| Shortcut | iTerm2 | Warp | Ghostty | Kitty (macOS) | VS Code | Cursor (extra) | Zed | Claude Code CLI | tmux |
|----------|--------|------|---------|---------------|---------|----------------|-----|-----------------|------|
| **Cmd+A** | -- | Select all blocks | -- | -- | Select all | (same as VS Code) | Select all | -- | N/A (prefix-based) |
| **Cmd+B** | -- | Bookmark block | -- | -- | Toggle sidebar | Toggle sidebar | Toggle left dock | -- | -- |
| **Cmd+C** | Copy | Copy | Copy | Copy | Copy | (same) | Copy | -- | -- |
| **Cmd+D** | **Split vertically** | **Split pane right** | **New split right** | -- | **Add selection to next find match** | (same as VS Code) | **Select next occurrence** | -- | -- |
| **Cmd+E** | -- | -- | -- | -- | -- (unbound by default) | -- | Buffer search (use selection) | -- | -- |
| **Cmd+F** | Find | Find | -- | Find (Cmd+F) | Find | (same) | Find | -- | -- |
| **Cmd+G** | -- | Find next occurrence | -- | Browse last cmd output | Find next / Go to line | (same) | Search: select next match | -- | -- |
| **Cmd+H** | -- | -- | -- | -- | Replace | (same) | -- | -- | -- |
| **Cmd+I** | -- | Reinput commands | -- | -- | Trigger suggestion | **Open Composer (AI)** | -- | -- | -- |
| **Cmd+J** | **Jump to mark** | -- | -- | -- | **Toggle panel** | (same as VS Code) | **Toggle bottom dock** | -- | -- |
| **Cmd+K** | **Clear buffer** | **Clear blocks** | **Clear screen** | -- | **Chord prefix** (Cmd+K then...) | **Inline AI edit** | **Clear (terminal) / chord prefix** | -- | -- |
| **Cmd+L** | -- | Focus terminal input | -- | -- | Select current line | **Open AI Chat** | -- | -- | -- |
| **Cmd+M** | **Set mark** | -- | -- | -- | -- (unbound) | -- | **Minimize window** | -- | -- |
| **Cmd+N** | -- | -- | **New window** | New OS window | New file | (same as VS Code) | New file | -- | -- |
| **Cmd+O** | -- | File search | -- | -- | Open file | (same) | Open folder | -- | -- |
| **Cmd+P** | -- | Command palette | -- | -- | Quick open / go to file | (same) | -- | -- | -- |
| **Cmd+Q** | Quit | Quit | Quit | -- | Quit | Quit | Quit | -- | -- |
| **Cmd+R** | -- | -- | **Clear screen** | **Resize window** | **Open recent** | (same as VS Code) | **Toggle right dock** | -- | -- |
| **Cmd+S** | -- | -- | -- | -- | Save | (same) | Save | -- | -- |
| **Cmd+T** | New tab | New tab | New tab | New tab | Show all symbols | (same) | -- | -- | -- |
| **Cmd+U** | -- | -- | -- | -- | Undo cursor | (same) | -- | -- | -- |
| **Cmd+V** | Paste | Paste | Paste | Paste | Paste | (same) | Paste | -- | -- |
| **Cmd+W** | Close tab/window | Close tab | Close surface | Close window | Close editor | (same) | Close | -- | -- |
| **Cmd+X** | -- | -- | -- | -- | Cut | (same) | Cut | -- | -- |
| **Cmd+Z** | -- | Undo | -- | -- | Undo | (same) | Undo | -- | -- |
| **Cmd+\\** | **Find cursor** | **Warp Drive** | -- | -- | -- | -- | **Split right** | -- | -- |
| **Cmd+,** | -- | Settings | Config | Edit config | Settings | Settings | Settings | -- | -- |

---

## Cmd+Shift+Letter Shortcuts

| Shortcut | iTerm2 | Warp | Ghostty | Kitty (macOS) | VS Code | Cursor (extra) | Zed |
|----------|--------|------|---------|---------------|---------|----------------|-----|
| **Cmd+Shift+C** | Copy mode | Copy command | -- | -- | -- | -- | Collab panel |
| **Cmd+Shift+D** | Split horizontally | Split pane down | New split down | Close window | Show debug | -- | Duplicate selection |
| **Cmd+Shift+E** | -- | -- | -- | -- | Show explorer | -- | Project panel |
| **Cmd+Shift+F** | -- | -- | -- | -- | Find in files | -- | Find in project |
| **Cmd+Shift+G** | -- | Find previous | -- | -- | Find previous | -- | Search: select prev match |
| **Cmd+Shift+H** | -- | -- | -- | -- | Replace in files | -- | -- |
| **Cmd+Shift+I** | -- | Reinput as root | -- | Set tab title | -- | **Full-screen Composer** | -- |
| **Cmd+Shift+J** | Scrollback to file | -- | -- | -- | Toggle search details | **Cursor Settings** | -- |
| **Cmd+Shift+K** | -- | Clear selected lines | -- | -- | Delete line | -- | -- |
| **Cmd+Shift+L** | -- | -- | -- | Next layout | Select all occurrences | **Open AI Chat w/ selection** | Select all matches |
| **Cmd+Shift+M** | -- | -- | -- | -- | Show problems panel | -- | Diagnostics |
| **Cmd+Shift+N** | -- | -- | -- | -- | New window | (same) | New window |
| **Cmd+Shift+O** | -- | -- | -- | -- | Go to symbol | (same) | Go to symbol |
| **Cmd+Shift+P** | -- | Nav palette | -- | -- | Command palette | (same) | Command palette |
| **Cmd+Shift+R** | -- | -- | -- | -- | -- | -- | Spawn task |
| **Cmd+Shift+S** | -- | Share block | -- | -- | Save as | (same) | Save as |
| **Cmd+Shift+T** | -- | Reopen closed tab | -- | -- | Reopen closed editor | (same) | Reopen closed item |
| **Cmd+Shift+U** | -- | -- | -- | -- | Show output panel | -- | -- |
| **Cmd+Shift+V** | -- | -- | -- | -- | Markdown preview | -- | -- |
| **Cmd+Shift+W** | -- | -- | Close window | -- | Close window | -- | Close window |
| **Cmd+Shift+X** | -- | -- | -- | -- | Show extensions | -- | -- |
| **Cmd+Shift+Z** | -- | Redo | -- | -- | Redo | (same) | Redo |

---

## Alt/Option Shortcuts

| Shortcut | iTerm2 | Warp | Ghostty | Kitty | VS Code | Cursor | Zed | Claude Code CLI |
|----------|--------|------|---------|-------|---------|--------|-----|-----------------|
| **Alt+arrows** | -- | Bookmark up/down | -- | -- | -- | -- | -- | -- |
| **Alt+Left/Right** | Word nav (if configured) | -- | Word nav (macOS default) | -- | -- | -- | -- | -- |
| **Alt+B** | Word backward | -- | -- | -- | -- | -- | -- | Word backward |
| **Alt+F** | Word forward | -- | -- | -- | -- | -- | -- | Word forward |
| **Alt+P** | -- | -- | -- | -- | -- | -- | -- | **Switch model** |
| **Alt+N** | -- | -- | -- | -- | -- | -- | -- | -- |
| **Alt+T** | -- | -- | -- | -- | -- | -- | -- | **Toggle thinking** |
| **Alt+Y** | -- | -- | -- | -- | -- | -- | -- | Cycle paste history |
| **Alt+Z** | -- | -- | -- | -- | Toggle word wrap | -- | -- | -- |
| **Alt+1-9** | -- | -- | Tab navigation | -- | -- | -- | -- | -- |
| **Alt+Click** | Cursor jump | -- | -- | -- | Multi-cursor | -- | -- | -- |

---

## tmux Default Key Bindings (prefix Ctrl+B, then key)

tmux uses a completely different model: prefix key (Ctrl+B by default) followed by a command key. It does not use Cmd shortcuts.

| After Prefix | Action |
|-------------|--------|
| d | Detach session |
| c | Create window |
| n | Next window |
| p | Previous window |
| w | List windows |
| , | Rename window |
| & | Kill window |
| % | Split vertical |
| " | Split horizontal |
| o | Swap panes |
| x | Kill pane |
| z | Toggle pane zoom |
| { | Move pane left |
| } | Move pane right |
| Space | Toggle layouts |
| q | Show pane numbers |
| t | Display clock |
| ? | List all shortcuts |
| s | List sessions |
| $ | Name session |
| 0-9 | Select window by number |

---

## Windows Platform Conventions

TUI Commander is cross-platform: macOS uses Cmd, Windows/Linux use Ctrl. This section maps our shortcuts to their Ctrl equivalents and identifies Windows-specific conflicts.

### Windows System Reserved Shortcuts

These are reserved by Windows itself and **must never be used**:

| Shortcut | Windows System Action |
|----------|----------------------|
| Ctrl+C | Copy (also: interrupt in terminals) |
| Ctrl+V | Paste |
| Ctrl+X | Cut |
| Ctrl+A | Select all |
| Ctrl+Z | Undo |
| Ctrl+Y | Redo (Windows convention!) |
| Ctrl+Alt+Delete | Security screen |
| Win+key combos | All reserved for OS (Start, Settings, Lock, etc.) |
| Ctrl+Shift+Esc | Task Manager |
| Alt+Tab | Window switcher |
| Alt+F4 | Close window |
| F11 | Toggle fullscreen (browsers, Explorer) |

**Critical: Ctrl+Y = Redo on Windows.** This is deeply ingrained muscle memory for Windows users. While macOS uses Cmd+Shift+Z for redo, Windows universally uses Ctrl+Y. This makes Ctrl+Y (the Windows equivalent of Cmd+Y) a **bad choice** for diff toggle on Windows, despite being "safe" on macOS.

### Windows Terminal Shortcuts

| Shortcut | Windows Terminal Action |
|----------|----------------------|
| Ctrl+Shift+T | New tab |
| Ctrl+Shift+D | Duplicate tab |
| Ctrl+Shift+W | Close tab |
| Ctrl+Shift+N | New instance |
| Ctrl+Shift+P | Command palette |
| Ctrl+Shift+F | Find |
| Ctrl+, | Settings |
| Alt+Shift+D | Split pane (auto direction) |
| Alt+Shift+Plus | Split pane right |
| Alt+Shift+Minus | Split pane down |
| Ctrl+Alt+1-9 | Switch to tab N |
| Alt+arrows | Move focus between panes |

### VS Code on Windows (Ctrl instead of Cmd)

| macOS (Cmd) | Windows (Ctrl) | VS Code Action | Conflict with TUI? |
|-------------|----------------|----------------|---------------------|
| Cmd+D | Ctrl+D | Add selection to next find match | YES - same conflict |
| Cmd+E | Ctrl+E | Quick open recent | Low - different action than macOS |
| Cmd+G | Ctrl+G | Go to line | YES |
| Cmd+J | Ctrl+J | Toggle panel | YES |
| Cmd+K | Ctrl+K | Chord prefix | YES |
| Cmd+M | Ctrl+M | Toggle Tab key moves focus | Different from macOS Cmd+M! |
| Cmd+N | Ctrl+N | New file | YES |
| Cmd+R | Ctrl+R | Open recent | YES |
| Cmd+\\ | Ctrl+\\ | Split editor | Similar semantics (good) |
| Cmd+Shift+D | Ctrl+Shift+D | Show debug / run panel | Moderate |
| Cmd+Shift+G | Ctrl+Shift+G | Source control panel | Different from macOS! |
| Cmd+Shift+L | Ctrl+Shift+L | Select all occurrences | YES |

**Key Windows-specific differences from macOS VS Code:**
- **Ctrl+M** in VS Code Windows = "Toggle Tab key moves focus" (not minimize like macOS Cmd+M). This means Ctrl+M is actually available for our use on Windows without system conflict.
- **Ctrl+Shift+G** in VS Code Windows = Source Control panel (not "find previous" like macOS). Our use for "git operations panel" is actually semantically aligned!

### Cross-Platform Mapping of Our Shortcuts

| macOS | Windows/Linux | TUI Feature | Windows Conflicts |
|-------|---------------|-------------|-------------------|
| Cmd+D | Ctrl+D | Diff panel | VS Code multi-select, shell EOF |
| Cmd+E | Ctrl+E | File browser | VS Code quick open recent |
| Cmd+G | Ctrl+G | Lazygit | VS Code go to line |
| Cmd+J | Ctrl+J | Task queue | VS Code toggle panel |
| Cmd+K | Ctrl+K | Prompt library | VS Code chord prefix |
| Cmd+M | Ctrl+M | Markdown panel | Low conflict (VS Code: tab focus toggle) |
| Cmd+N | Ctrl+N | Ideas panel | VS Code/browsers new file/window |
| Cmd+R | Ctrl+R | Run command | VS Code open recent, browsers reload |
| Cmd+\\ | Ctrl+\\ | Split | VS Code split editor (same semantics) |
| Cmd+Shift+G | Ctrl+Shift+G | Git ops panel | VS Code source control (similar!) |
| Cmd+Shift+L | Ctrl+Shift+L | Lazygit split | VS Code select all occurrences |
| Cmd+Shift+T | Ctrl+Shift+T | Reopen tab | Same semantics everywhere (good) |
| Alt+arrows | Alt+arrows | Navigate panes | Windows Terminal pane nav (same!) |

---

## Analysis

### 1. Cmd+D Conflict Analysis

**Cmd+D is heavily used across all tools:**

| Tool | Cmd+D Action | Severity |
|------|-------------|----------|
| iTerm2 | Split vertically | HIGH - core feature |
| Warp | Split pane right | HIGH - core feature |
| Ghostty | New split right | HIGH - core feature |
| VS Code | Add selection to next find match | HIGH - used constantly |
| Cursor | (same as VS Code) | HIGH |
| Zed | Select next occurrence | HIGH |
| Kitty | -- (unbound) | No conflict |
| Claude Code | Ctrl+D = exit session | LOW (Ctrl, not Cmd) |

**Verdict: Cmd+D is the WORST possible choice for "toggle diff panel."** Every terminal uses it for splitting, and every code editor uses it for multi-select. Users embedded in any of these tools will have muscle memory conflicts.

### 2. Other Current TUI Commander Conflicts

| Our Shortcut | Conflicts With |
|-------------|----------------|
| **Cmd+D** (diff) | iTerm2/Warp/Ghostty (split), VS Code/Zed (multi-select) |
| **Cmd+E** (file browser) | Zed (buffer search). Low conflict otherwise |
| **Cmd+G** (lazygit) | Warp (find next), VS Code (find next/go to line), Zed (select next match) |
| **Cmd+J** (task queue) | iTerm2 (jump to mark), VS Code (toggle panel), Zed (toggle bottom dock) |
| **Cmd+K** (prompt library) | iTerm2/Warp/Ghostty (clear), VS Code (chord prefix), Cursor (inline AI), Zed (clear/chord) |
| **Cmd+M** (markdown) | iTerm2 (set mark), Zed (minimize) |
| **Cmd+N** (ideas) | Ghostty/Kitty (new window), VS Code/Zed (new file) |
| **Cmd+R** (run command) | Ghostty (clear), Kitty (resize), VS Code (open recent), Zed (toggle right dock) |
| **Cmd+\\** (split) | iTerm2 (find cursor), Warp (Warp Drive), Zed (split right) |
| **Cmd+Shift+G** (git ops) | Warp/VS Code (find previous) |
| **Cmd+Shift+L** (lazygit split) | VS Code/Zed (select all occurrences), Cursor (AI chat w/ selection) |
| **Cmd+Shift+T** (reopen tab) | VS Code/Warp/Zed (reopen closed tab) -- GOOD, same semantics! |

### 3. "Safe" Cmd/Ctrl+Letter Combos (cross-platform analysis)

These combos are NOT used by any of the researched tools (or used by at most 1 tool for a minor feature). **Both macOS and Windows equivalents are checked.**

| macOS | Windows | macOS Status | Windows Status | Verdict |
|-------|---------|-------------|----------------|---------|
| **Cmd+E** | Ctrl+E | Only Zed (buffer search) | VS Code (quick open recent) | MODERATE - low conflict on both |
| **Cmd+H** | Ctrl+H | macOS: hide app. AVOID | VS Code: replace | AVOID (macOS system) |
| **Cmd+U** | Ctrl+U | Only VS Code (undo cursor) | VS Code (undo cursor) | MODERATE |
| **Cmd+Y** | Ctrl+Y | Unused on macOS | **Redo on Windows!** System-level | **AVOID** (Windows redo) |
| **Cmd+;** | Ctrl+; | Unused | Unused | SAFE cross-platform |
| **Cmd+'** | Ctrl+' | Unused | VS Code (toggle terminal) | MODERATE |

Truly safe Cmd+Shift / Ctrl+Shift combos:

| macOS | Windows | Status |
|-------|---------|--------|
| **Cmd+Shift+R** | Ctrl+Shift+R | Only Zed (spawn task). Mostly free. |
| **Cmd+Shift+B** | Ctrl+Shift+B | Only Zed (outline panel). VS Code: build task. MODERATE. |

**System shortcuts to avoid:**
- macOS: Cmd+H (hide), Cmd+M (minimize), Cmd+Q (quit), Cmd+Tab (app switcher)
- Windows: Ctrl+Y (redo), Ctrl+C/V/X/A/Z (clipboard/undo), Ctrl+Shift+Esc (task manager), Alt+F4 (close)

### 4. Recommended Alternative for "Toggle Diff Panel" (currently Cmd+D)

**Option A: Cmd+Shift+D / Ctrl+Shift+D** -- "Show Debug" in VS Code (both platforms), "Duplicate tab" in Windows Terminal, "Split horizontal" in iTerm2/Warp/Ghostty. Moderate conflicts but less than Cmd+D. The VS Code "Debug" association is conceptually adjacent to "Diff." Windows Terminal's "Duplicate tab" is not commonly used.

**Option B: ~~Cmd+Y~~ ELIMINATED** -- While unused on macOS, Ctrl+Y is the **universal Redo shortcut on Windows**. This would create a severe conflict for Windows users. Not viable for a cross-platform app.

**Option C: Cmd+U / Ctrl+U** -- Only VS Code uses it (undo cursor). Low conflict on both platforms. But "U" has no mnemonic connection to "diff."

**Option D: Keep Cmd+D but document the conflict** -- Users in TUI Commander are not simultaneously in VS Code's editor or iTerm2's terminal; TUI Commander IS the terminal. However, users with iTerm2 muscle memory will instinctively hit Cmd+D to split.

**Option E: Cmd+; / Ctrl+;** -- Unused across ALL tools on ALL platforms. Zero conflicts. No system-level reservation. Ergonomically less discoverable but completely safe.

**Recommendation: Cmd+Shift+D / Ctrl+Shift+D** is the pragmatic choice. It's the "diff/debug" mental model (VS Code uses it for Debug/Run panel, which is conceptually adjacent). Terminal emulators use Cmd+Shift+D for horizontal split / duplicate tab, but TUI Commander already uses Cmd+\\ for splitting. The Windows Terminal "duplicate tab" conflict is minor. If you want absolute zero conflicts, use **Cmd+; / Ctrl+;**.

### 5. Alt+P and Alt+N Analysis

| Shortcut | Used By |
|----------|---------|
| **Alt+P** | Claude Code CLI (switch model). No other tool uses it. |
| **Alt+N** | Unused across all tools. SAFE. |
| **Alt+T** | Claude Code CLI (toggle thinking). No other tool uses it. |

Alt+letter shortcuts are generally safe territory because:
- Terminal emulators pass them through to the shell
- Code editors rarely use them (Zed recently removed Alt+letter defaults for keyboard layout compatibility)
- Claude Code CLI uses a few (Alt+P, Alt+T) but these are in its own input context

### 6. System Shortcuts to Avoid (Cross-Platform)

#### macOS

| Shortcut | macOS System Action |
|----------|-------------------|
| Cmd+H | Hide application |
| Cmd+M | Minimize window |
| Cmd+Q | Quit application |
| Cmd+Tab | App switcher |
| Cmd+Space | Spotlight |
| Cmd+, | Preferences (convention) |

**Note on Cmd+M:** TUI Commander currently uses Cmd+M for "toggle markdown panel." This conflicts with macOS's "Minimize window" system shortcut. Tauri may intercept this before it reaches the app. Worth testing.

#### Windows

| Shortcut | Windows System Action |
|----------|----------------------|
| Ctrl+Y | Redo (universal Windows convention) |
| Ctrl+Shift+Esc | Task Manager |
| Alt+F4 | Close window |
| Win+anything | OS-reserved (Start menu, Snap, Settings, Lock, etc.) |
| Ctrl+Alt+Delete | Security screen |
| Ctrl+C | Copy / terminal interrupt (dual meaning) |
| F11 | Toggle fullscreen (browsers, Explorer) |

**Note on Ctrl+M:** Unlike macOS Cmd+M (minimize), Windows Ctrl+M has no system reservation. In VS Code it toggles "Tab key moves focus" — a rarely used feature. Our Cmd+M → Ctrl+M mapping for "toggle markdown panel" is actually safe on Windows.

#### Linux

Linux follows Windows conventions (Ctrl-based) but has fewer system reservations. Desktop environments (GNOME, KDE) use Super (Win) key for OS functions. Ctrl+Alt+T is conventionally "open terminal" in GNOME/Ubuntu. Ctrl+Alt+Delete varies by distro.

---

## Sources

### macOS Tools
- [iTerm2 Shortcuts - DefKey](https://defkey.com/iterm-shortcuts)
- [iTerm2 Shortcuts - KeyCombiner](https://keycombiner.com/collections/iterm2/)
- [Warp Keyboard Shortcuts](https://docs.warp.dev/getting-started/keyboard-shortcuts)
- [Ghostty Keybindings Config](https://ghostty.org/docs/config/keybind)
- [Ghostty Shortcuts Gist](https://gist.github.com/hensg/43bc71c21d1f79385892352a390aa2ca)
- [Ghostty Shortcuts & Commands Gist](https://gist.github.com/robbyrob42/c139fb451e0055c63acbce4db9372db3)
- [Kitty Overview & Shortcuts](https://sw.kovidgoyal.net/kitty/overview/)

### Cross-Platform Editors
- [VS Code macOS Shortcuts PDF](https://code.visualstudio.com/shortcuts/keyboard-shortcuts-macos.pdf)
- [VS Code Windows Shortcuts PDF](https://code.visualstudio.com/shortcuts/keyboard-shortcuts-windows.pdf)
- [VS Code Shortcuts - QuickRef](https://quickref.me/vscode.html)
- [VS Code Shortcuts - WebReference](https://webreference.com/cheat-sheets/vscode/)
- [Cursor Shortcuts - cursor101.com](https://cursor101.com/cursor/cheat-sheet)
- [Cursor Shortcuts Guide - Refined](https://refined.so/blog/cursor-shortcuts-guide)
- [Zed Default macOS Keymap (source)](https://github.com/zed-industries/zed/blob/main/assets/keymaps/default-macos.json)
- [Zed Cheat Sheet](https://cheatsheets.zip/zed)

### Windows
- [Windows Terminal Keyboard Shortcuts](https://learn.microsoft.com/en-us/windows/terminal/customize-settings/actions)
- [Windows Keyboard Shortcuts](https://support.microsoft.com/en-us/windows/keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec)

### CLI Tools
- [Claude Code CLI Shortcuts - DefKey](https://defkey.com/claude-code-cli-shortcuts)
- [Claude Code Keybindings Docs](https://code.claude.com/docs/en/keybindings)
- [tmux Cheatsheet Gist](https://gist.github.com/MohamedAlaa/2961058)
