# Terminal Features & Keyboard Shortcuts

Consolidated reference for all terminal behaviors, keyboard shortcuts, and configurable features.

## Keyboard Shortcuts

### Terminal Management

| Shortcut | Action | Notes |
|----------|--------|-------|
| Cmd+T | New terminal tab | |
| Cmd+W | Close tab/pane | Closes active split pane, or tab if no split |
| Cmd+Shift+T | Reopen closed tab | Restores last 10 closed tabs |
| Cmd+1–9 | Switch to tab N | First 9 tabs only |
| Ctrl+Tab | Next tab | NSEvent monitor on macOS |
| Ctrl+Shift+Tab | Previous tab | NSEvent monitor on macOS |

### Terminal Content

| Shortcut | Action | Notes |
|----------|--------|-------|
| Cmd+L | Clear terminal | Sends Ctrl+L to shell (clear screen) |
| Cmd+K | Clear scrollback | Clears entire scrollback buffer (common terminal convention) |
| Cmd+C | Copy selection | |
| Cmd+V | Paste | |
| Cmd+F | Find in terminal | Search overlay with match highlighting |
| Cmd+G | Find next match | |
| Shift+Cmd+G | Find previous match | |

### Scrolling

| Shortcut | Action |
|----------|--------|
| Cmd+Home | Scroll to top of scrollback |
| Cmd+End | Scroll to bottom |
| Shift+PageUp | Scroll one page up |
| Shift+PageDown | Scroll one page down |
| Wheel / two-finger | Scroll the scrollback (smooth) |
| Shift+Wheel | Force scrollback scroll, never sent to the app |

**Wheel vs. mouse-reporting apps.** When an app enables mouse tracking, the wheel
is forwarded to it as SGR mouse codes whenever mouse reporting is on, in **either**
buffer — the alternate screen (vim, lazygit, htop) and inline fullscreen TUIs in the
main buffer alike (e.g. `grok --no-alt-screen`, or Claude Code, which enables mouse
tracking without switching to the alt screen). Hold **Shift** to force scrollback
scrolling regardless of mouse mode (consistent with Shift bypassing mouse reporting
for clicks/selection). The scrollbar thumb always moves TUIC history.

**Quantization (forwarded to an app).** A physical wheel notch or trackpad flick
delivers a burst of many small-delta DOM wheel events (macOS reports the whole
momentum decay, not just the initial motion). The forwarded path accumulates those
pixel deltas and emits one SGR notch per full line height crossed, capped at 3 notches
per event in pixel mode (to match native terminal behavior for a single discrete mouse
click); the residual resets on direction reversal and when the gesture goes idle
(200 ms). Line-mode devices (`deltaMode` 1) are rounded to whole lines and clamped to
the viewport row count; page-mode devices (`deltaMode` 2) emit `viewportRows - 1`
notches per event.

**Scrollback scrolling.** The scrollback path (no mouse reporting, or Shift held) is
not quantized: pixel deltas, normalized from line/page mode, drive smooth fractional
scrolling. Gesture acceleration ramps from 0.5x (first screenful of travel) to 1.0x
(after 2 screens) and is capped at 2.0x (after 4 screens); it resets on direction
reversal and when the gesture ends.

### Split Panes

| Shortcut | Action | Notes |
|----------|--------|-------|
| Cmd+\ | Split vertically | Side-by-side, max 4 panes |
| Cmd+Alt+\ | Split horizontally | Stacked |
| Cmd+Shift+Enter | Maximize/restore pane | Toggle zoom on active pane |
| Alt+Arrow Left/Right | Navigate vertical panes | |
| Alt+Arrow Up/Down | Navigate horizontal panes | |

### Panels

| Shortcut | Action |
|----------|--------|
| Cmd+[ | Toggle sidebar |
| Cmd+, | Settings |
| Cmd+E | File browser |
| Cmd+Shift+M | Markdown panel |
| Cmd+Alt+N | Notes/ideas panel |
| Cmd+O | Open file picker |
| Cmd+N | New file (picker for name + location) |
| Cmd+J | Task queue |
| Cmd+B | Quick branch switch |
| Cmd+G | Branches tab |
| Cmd+Shift+D | Git operations panel |
| Cmd+Shift+E | Error log |
| Cmd+Shift+A | Activity dashboard |
| Cmd+Shift+W | Worktree manager |
| Cmd+Shift+M | MCP servers popup |
| Cmd+Shift+G | Diff scroll view |
| Cmd+? | Help panel |

### Navigation

| Shortcut | Action |
|----------|--------|
| Cmd+P | Command palette |
| Cmd+Shift+K | Prompt library |
| Cmd+Shift+R | Run saved command |
| Cmd+R | Reload active web/preview tab |
| Cmd+Shift+F | Search file contents |
| Cmd+Ctrl+1–9 | Quick branch switch (hold Cmd+Ctrl, press number); in the Global Workspace it switches to tab N |

### Zoom

| Shortcut | Action |
|----------|--------|
| Cmd+= / Cmd++ | Zoom in |
| Cmd+- | Zoom out |
| Cmd+0 | Reset zoom |

## Terminal Behaviors

### Copy on Select
Auto-copies selected text to clipboard when text is selected in terminal. Configurable in settings (`copy_on_select`, default: on).

### URL Click
Cmd+Click on URLs in terminal output opens them in the system browser. URL detection handled by the native terminal link parser.

### File Path Click
Clickable file paths in terminal output (absolute and relative paths with known extensions). Opens in IDE or markdown viewer.

### Tab Features
- **Middle-click** closes tab
- **Right-click** context menu: Close, Close Other, Close Right, Rename, Detach to Window, Move to Worktree, Pin/Unpin, Copy Path
- **Drag-and-drop** to reorder tabs
- **Double-click** tab title to rename
- **Unseen activity badge** when tab has new output

### Split Panes
- Max 4 panes per branch
- Drag divider to resize
- Flexible ratios preserved across layout changes
- Modes: "separate" (independent tab bars) or "unified" (shared tab bar)

## Configurable Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `copy_on_select` | `true` | Auto-copy terminal selection to clipboard |
| `confirm_before_quit` | `true` | Show dialog when quitting with active terminals |
| `confirm_before_closing_tab` | `true` | Show dialog when closing tab with running process |
| `split_tab_mode` | `"separate"` | Tab bar mode for split panes |
| `tab_ordering_mode` | `"grouped-by-type"` | Tab ordering: "grouped-by-type", "terminals-first", "free" |
| `intent_tab_title` | `true` | Show agent intent as tab title |
| `suggest_followups` | `true` | Show suggested follow-up actions from agents |
| `bell_style` | `"visual"` | Terminal bell: "none", "visual", "sound", "both" |
| `prevent_sleep_when_busy` | `false` | Prevent system sleep while terminal is busy |
