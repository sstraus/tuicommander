# Terminal Features & Keyboard Shortcuts

On desktop, returning from Chat to CLI focuses the terminal input. You can type immediately after using the switch, closing Chat Compose, answering in CLI, or falling back from unavailable Chat. Only the active terminal restores focus, and an automatic fallback does not take the caret from an editable field outside that terminal.

Session Chat shows a pulsing **Working…** row after Send and while the terminal reports busy. It disappears when the terminal reports idle or awaits a question or permission answer. Desktop and mobile use the existing session state; Chat does not infer activity from the conversation text.

Consolidated reference for all terminal behaviors, keyboard shortcuts, and configurable features.

The desktop CLI | Chat switch uses shared toolbar density. In both modes it is centred on the Context header and stays above its bottom divider, without adding a separate row. Without Context, both modes reserve one shared control row so content cannot run beneath the switch. Switching modes keeps the switch and content at the same height. At touch widths up to 600px, the Context header and collapsed bar reserve a 48px row for the shared 44px switch; expanded Context keeps the same header clearance.

Terminal Find overlays the grid below the Context and CLI/Chat switch row. Search controls wrap within narrow panes without covering the view switch.

## Answer gutters after resize

When scrollback reflow is disabled, widening a terminal keeps existing text in
its original physical rows. Green answer gutters still cover the retained
soft-wrapped continuation rows. This does not enable reflow or join the text
into wider rows. MCP session creation without dimensions starts at 80 columns
and 24 rows; callers can supply dimensions before starting output.

## Mobile composer focus

The composer border highlights only while its textarea has focus. Tapping the mic releases editor focus without changing the insertion range. Recording uses the red mic icon, not an extra accent border. Keyboard navigation retains the mic focus ring. The blue slash key marks an available action, not retained focus.

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

Desktop terminal Chat and AI Chat use the shared scale in `styles/chat-tokens.css`: 14px plus the terminal font-size delta from the configured default, bounded to 8–32px. Terminal Chat reads its own terminal; AI Chat reads the active terminal and uses zero delta when none is active. Reset restores the 14px reading size. Mobile retains its device-local reading preference.

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
Clickable file paths in terminal output resolve against the terminal working directory when relative. Sentence periods are excluded from heuristic path targets, including paths in soft-wrapped lines. Existing files open in their viewer or editor; directories open in the File Browser.

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

### Collapsed Compose queue count

The collapsed handle has transparent chrome and muted text at rest. Hover or keyboard focus reveals its background and border. The label and queue count stay fully opaque.

The CLI Compose button shows a muted count pill for all pending FIFO entries,
including user commands and TUIC messages. It hides at zero and follows the
backend session state without opening the editor. Hover or keyboard focus loads
the command/message breakdown; the count remains available if details fail.

### Compose editor size

Terminal Compose uses the shared auto-grow control bounds: one control line when empty (36px desktop, 44px touch), intrinsic CodeMirror growth for newlines and wrapping, and the existing 96px editor maximum. Longer drafts scroll inside the editor. Clearing or successfully submitting a draft restores the minimum. The queue list keeps its independent height cap.

### Compose editor shortcuts

The newline legend shows only Ctrl+Enter. A panel-relative CSS container query hides shortcut hints at widths up to 640px, including split terminals inside a wide window. The footer does not wrap its hints around the action controls.

In the terminal Compose panel, Enter sends the draft. Ctrl+Enter and Shift+Enter insert a new line. Shift+Ctrl+Enter queues for the next idle moment when an agent is detected. Enter used to confirm an IME candidate does not submit the draft. These shortcuts do not change native terminal keybar controls.

## Mobile stream recovery

Mobile CLI output releases a terminal subscription when the transport reports exit, including retry exhaustion. Visibility resume starts a fresh subscription if none remains and the session poll has not confirmed the session ended. Retired stream callbacks cannot close a newer subscription or change its connection error state. Handshake retries retain their three-second initial delay and 30-second backoff cap; hidden pages do not start a scheduled retry.

### Unsized pane diagnostics

Canvas sizing retries run only for a visible pane connected to the layout. A pane
that stays unsized for 120 animation frames logs a warning. Hiding or closing the
pane cancels the retry; showing it starts a new measurement.

Restored PTY sessions attach their event listeners before the first layout pass.
The reconnect resize waits for that attachment and reuses successful listeners
instead of adding another API-error consumer. If the initial handshake failed,
reconnect retries the attachment once. Suspend detaches them; resume attaches the replacement PTY once.

Claude native question cards in desktop PTY Chat and mobile keep wrapped answer
labels separate from the selected option's monospace preview. They retain the
question tabs and the native Chat about this action. Hidden options still use
explicit previous/next/select controls rather than guessed labels.

### Expanded Chat tool rows

Expanded tool rows use theme colours for Bash/execute commands, flags, strings and operators. Read, Edit and Write keep the tool name separate from the path; other tools retain readable text. The collapsed activity summary and row sizing are unchanged.

### Pending Chat prompts

In an open terminal Chat view, a submitted composer prompt appears immediately as a muted **Pending** bubble. Several prompts stay in send order while the agent works. A matching new user message from the native transcript replaces its bubble and removes older unmatched bubbles, since the CLI handles sends in order. A local slash command or rewritten prompt therefore cannot block later receipts. Adapter-redacted or truncated text can remain pending until a later matching prompt arrives or Chat closes. Failed sends remove their own feedback; ordinary activity and idle signals do not. This applies to the shared Claude, Codex, Grok and ego transcript presentation, but does not add queueing support to a CLI that lacks it. TUIC cannot preview prompts typed in the hidden CLI or sent outside this client. The CLI owns its queue; pending bubbles have no Cancel action and are not persisted when Chat closes.
