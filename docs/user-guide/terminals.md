# Terminal Features

Remote terminals replay their current viewport when attached or reconnected. A stream failure or stalled initial replay shows a persistent error toast; the client retries without requiring new terminal output. See [Remote Access](remote-access.md).

On desktop and mobile CLI views, adjacent `💬` answer paragraphs share one continuous green left bar without a background tint (Paper uses its darker success green for contrast), including blank rows between them. Mobile places this bar in a separate gutter outside the text box, so column-zero text stays clear and existing agent indentation stays unchanged. An unmarked prose paragraph after a blank row ends the answer; an uninterrupted indented continuation stays inside. The CLI gutter joins separate marked answer sections across internal blank rows and indented section headings when a later marked answer closes the gap. Trailing unmarked prose, unmarked list siblings, tools, prompts and status rows stay outside that gutter; extracted Answers and Chat keep their semantic paragraph boundaries. Lists, tables, quotations and fenced code remain attached to the answer. Fenced code keeps its inset and a separate Copy row above the text on desktop and mobile; the control does not cover wrapped lines. A non-answer output row, prompt or Codex `Worked for` duration footer ends the region. The desktop CLI blanks only the marker glyph cells; copy, selection and search keep the original text. Mobile omits the same protocol glyphs from displayed text while retaining ANSI styles and links; search still uses the original log text. Chat uses the same answer extent and strips every paragraph marker. A list item whose text starts with `💬` also opens an answer: hard-wrapped continuation rows and nested sub-items belong to it, but an unmarked sibling or dedented prose does not. The list bullet stays visible; only the answer marker is hidden. Standalone fenced terminal examples keep protocol-looking answer, intent and suggestion text literal, even when no marked answer precedes the fence. User prompt echoes do not open answer regions. A quoted marker inside uninterrupted pasted prose stays in the prompt. An agent output bullet, a column-zero answer marker, or a blank-separated answer marker begins output; indentation alone does not decide ownership. The viewport uses tracked prompt starts even when a long prompt begins outside its marker lookback. If that prefix is not cached, answer markers wait for bounded prompt-boundary reads rather than being inferred from the visible fragment. CLI marker classification examines at most 128 rows above the visible window and reuses unchanged row generations; an answer anchor farther above that window is not inferred from continuation prose.

Turning **Show scrollbar marks** off or on updates history ticks immediately, even when the terminal is idle. Search-match ticks remain visible when history marks are off.

The Context preview truncates before the CLI/Chat switch. Terminals without that switch use the full header width.

## Tablet Keyboard

Tap the terminal to focus its keyboard input. Touch and mouse input use the same field, including soft-keyboard text entry and repeated deletion. Primary mouse presses keep focus on that input without a temporary canvas focus change.

## Terminal Sessions

Each terminal tab runs an independent PTY (pseudo-terminal) session with your shell. Up to 50 concurrent sessions.

Files uploaded through the mobile picker live in a session directory under TUICommander’s data directory. New Claude and Gemini sessions receive read access at launch. Closing the terminal or ending its process removes these uploads; resumed sessions need files uploaded again. Uploads leave your repository and Git excludes untouched. Old `.tuic/attachments/` directories can be removed by hand. Confined ego sessions need an existing user/profile read root; the picker shows an access notice.

Accents sent separately from their base letters remain part of terminal text
when you scroll, select, copy or search. Search matches the exact characters
printed by the program: precomposed and decomposed spellings are not automatically
normalized into each other.

In the mobile session output, a small upward drag stops auto-follow. New output does not scroll the view until you return to the bottom. While following, opening the keyboard or resizing the viewport keeps the newest output visible. A reader who has scrolled up keeps their position through a viewport resize.

When the viewed agent exits, the mobile session returns to the shell: question
controls close, Chat returns to CLI, and the composer sends shell input. The CLI
view shows new shell output even when the terminal retains the old agent screen;
the old agent input is not restored into the composer. Fresh
session polling also clears stale agent controls if the output socket pauses or
reconnects. If the PTY ends, **Session ended** offers **Back** to the session list.

In the mobile session output, completed lines continue into history even after the terminal history fills, while the live region redraws below. Long text wraps to the phone width. A continuation
keeps the original line's leading spaces or tabs, so indented lists and code
remain readable. Claude tool-call status dots keep the same space while pulsing
or turning green, and wrapped continuations align after that space. Box-drawing
output keeps its horizontal scrolling layout.

### Creating Terminals

- **Cmd+T** — New terminal for the active branch
- **`+` button** on sidebar branch — Add terminal to specific branch
- **`+` button** on tab bar — New terminal tab

New terminals inherit the working directory from the active branch's worktree path.

Desktop frame receipts do not generate more terminal output. After a missed
frame, desktop receives one full frame and connected browsers retain their delta stream. Output
batching remains bounded. Input-related output that arrives during a frame send
keeps its bounded retry; this does not guarantee echo within one display frame.

### CWD Tracking (OSC 7)

When your shell reports directory changes via OSC 7, TUICommander updates the terminal's working directory in real time. If the new directory falls inside a different worktree, the terminal tab is automatically reassigned to the corresponding branch in the sidebar.

### Terminal Lifecycle

Terminals are **never unmounted** from the DOM. When you switch branches or tabs, terminals are hidden but remain alive. Switch back and your process, scroll position, and output are exactly as you left them.

### Closing Terminals

- **Cmd+W** — Close active tab (confirmation only when a user-launched process is running — e.g. Claude Code, htop, npm; idle shells and shells still loading .zshrc close immediately)
- **Middle-click** on tab — Close tab
- **Right-click → Close Tab** — Context menu
- **Right-click → Close Other Tabs** — Close all except this one
- **Right-click → Close Tabs to the Right** — Close tabs after this one

### Reopening Closed Tabs

- **Cmd+Shift+T** — Reopen the last closed tab
- Last 10 closed tabs are remembered with their name, font size, and working directory
- Reopened tabs start a fresh shell session in the original directory

### CLI / Chat view

A terminal that runs Claude Code, Grok, Codex or ego shows a **CLI | Chat** switch in its top-right corner. **Chat** replaces the grid with the conversation read from Claude's own session file: your prompts as bubbles (including follow-ups typed while Claude is busy), Claude's replies as text, and tool calls folded into one compact card each. Plumbing (hook output, system reminders) and empty thinking blocks are left out. The desktop switch has its own row in Chat, so it does not cover the first prompt even when the context bar is hidden. Compose opens and receives focus below the conversation. Reply with **Ctrl+Enter**, or queue follow-up work with **Shift+Ctrl+Enter**. Active native question and permission choices appear above Compose. Select a named choice to send its native keys to that terminal, without sending or clearing your Compose draft. Questions without exported choices show **Answer in CLI**; Codex free-form alternatives also return to CLI. PTY Chat is separate from ACP AI Chat. The grid keeps running while hidden, so scrollback and selection are unchanged. Prompts have a theme-coloured background in Chat and a reading width of up to 80 characters; the CLI grid does not tint submitted prompts.

For ego, configure an absolute **ego executable** in Settings that supports local transcript v1, including `--after`. This is the executable Chat uses, even if your shell launches a newer ego from another path. A failed export reports the configured path and exit status; check that Settings points to the transcript-capable binary. TUICommander binds the foreground ego process to the session whose lease names that PID, then reads `ego export SESSION --format transcript --after SEQ` from the same `EGO_HOME`. It does not select the newest session in the folder or read ego's private event files. The existing Chat ticker follows committed entries; message previews are omitted and completed answers appear once. Tool starts and results share a card. Goal continuations and compaction checkpoints appear as system notices, not user prompts. A new session binding resets the conversation rather than joining unrelated histories. Missing binding, unsupported export versions and unreadable sessions return to CLI with a reason. ego Chat is available immediately after a page reload, including while a native question or permission prompt is pending. It does not wait for the frontend to rediscover a session ID. The backend still checks the live process and lease before reading. ego Chat is also available on mobile through the same transcript lifecycle and existing event stream.

Notice cards preserve line breaks between their heading and body. Harness inbox wakes are hidden in terminal Chat. Interruption notices remain compact chips. Images without retained image data appear as attachment chips. Consecutive thinking blocks share one disclosure. Answer blocks in the same turn share one reading card, limited to about 96 characters wide. Only the answer blocks selected by the CLI boundary rule have a green bar; unmarked prose after a blank row and later status text remain outside it on desktop and mobile. **Copy** in its top-right corner appears on hover or keyboard focus and copies the whole answer. Intermediate status prose remains separate and uses the normal readable text colour. Answer lists retain their list indent so their markers stay to the right of the green bar. Tool calls keep their single collapsed activity row; Claude Bash uses the agent-written description when available, while other tools and agents retain the backend title. Pasted CLI calls followed by a terminal output marker appear in a monospace block showing two lines; **+ N lines** expands the rest. Ordinary multiline prose stays as prose. Historical tool rows show status without a duration. Turn timestamps do not reach the frontend, so time separators are omitted. The Compose handle is available in CLI view.

In Chat, Compose stays docked and open after sending or queueing, clears the submitted text, and keeps focus for the next message. It cannot be closed or unpinned while Chat is active. **Esc** returns to CLI for terminal interaction; the Compose shortcut focuses the Chat input. Switching back to CLI restores that tab's previous Compose open and pin state. Unsent drafts survive switching in either direction, including text entered while a send or queue is still pending.

On mobile, Codex and Grok also offer **CLI | Chat** in the session header. The backend must confirm the exact Codex native-hook or Grok registry transcript binding; a missing binding returns to CLI with a reason. The existing question controls, keyboard bar and composer keep targeting the same PTY. Select **CLI** for native terminal interaction.

Local agents started through `tuic agent spawn` or HTTP remain eligible for Chat. HTTP/MCP creation controls worker-tab styling, cleanup and completion sounds; it does not mean that the PTY runs on another machine. A session owned by a configured remote connection remains unavailable to Chat.

**Chat** is disabled, with the reason as its tooltip, when the terminal has no agent, the agent has no supported transcript adapter, or a required frontend binding is missing. Claude and ego check their transcript binding in the backend when Chat opens, so a missing saved session ID does not disable their switch. If the agent exits or the binding is lost while Chat is open, the terminal returns to CLI with a one-line notice.

Chat applies each transcript snapshot as one update on desktop and mobile. Opening Chat or switching back from CLI keeps the full history without rendering each partial replay. Chat follows new transcript entries while the agent writes. When you scroll up and more content arrives, **New text below** appears above Compose. Select it to return to the bottom and resume following. Reaching the bottom by hand also clears it. Search jumps show no indicator until later content arrives. Turn the indicator off in **Settings → Appearance → Chat**; this preference is saved on the current client. A replaced transcript resets the displayed conversation, including when the replacement keeps the same file name and size.

Older recorded prompts with a prompt ID also appear when Claude did not record a human-origin field. Command echoes, tool results and sidechain conversations stay out of user bubbles. Image and PDF tool outputs show `[image]` and `[document]` markers; a model fallback shows a **Model changed** card.

### Suspending a Tab

**Right-click → Suspend Tab** ends the tab's process and its agent, freeing the memory and CPU they used, and keeps the tab in the tab bar marked `zz`. It is not auto-standby: auto-standby pauses an eligible idle process, which keeps its memory, and wakes it when you focus the tab. Agents running as separate foreground jobs inside an interactive shell are not auto-stopped, because the shell would reclaim their terminal. Remote viewing and typing prevent standby for a full timeout window after the last activity or disconnect. Desktop typing also wakes an already parked shell, even if the tab did not send another visibility change. A suspended tab holds no process at all.

- **Resume** — click **Resume** in the tab, or right-click → **Resume Tab**. A new session opens in the same folder. An agent tab runs the same resume command a tab gets after a TUICommander restart, so the agent continues its conversation. A plain shell tab opens a fresh shell.
- **Restart** — Rust saves suspended tabs before ending their processes. They stay suspended after a restart or frontend recreation, even when suspension happened with no UI open; they are never resumed automatically.
- **Refused while busy** — Suspend is disabled while the agent is working, a question waits for your answer, commands are queued, or a command runs in a plain shell, so no turn is cut. An agent tab with no resumable session is refused too.
- **MCP** — `session action=suspend session_id=<id>` does the same for a tab another agent wants to park, under the same refusals.

## Tab Management

### Tab Names

- Default naming: "Terminal 1", "Terminal 2", etc.
- **Double-click** a tab to rename it (inline editing)
- Press **Enter** to confirm, **Escape** to cancel
- Explicit custom names persist through reconnects and are never replaced by agent output
- Spawn-assigned agent labels are base names: an `intent: text (Title)` marker may replace them with the current work phase, even when its text wraps across many terminal rows
- Mobile CLI also keeps complete `intent:` text visible, rejoins terminal soft wraps, and tints every browser-wrapped row with the same theme warning colour.
- The canvas keeps `intent:` rows visible with a theme-aware warning tint for ego, Claude and Codex. It removes complete `suggest:` marker rows and their continuation rows from the displayed grid. They occupy no selectable line and are excluded from copied text, including selections that cross scrollback. Suggestion chips remain independent of this projection. When blank separators surround the removed block, one separator remains instead of two adjacent blank lines.
- Agent terminals show an expandable **Context** bar. It separates the model's current **Intent**, the orchestrator-owned **Assignment**, and the last substantial user **Prompt**. MCP-connected models are instructed to refresh intent at task start and whenever the material work phase changes
- Captured intent and prompt are recovered when reconnecting to a live session, including an idle agent. **Prompt** retains the most recent submission with at least ten words; shorter follow-ups do not replace it.

### Tab Reordering

Drag tabs to reorder them. Visual drop indicators show where the tab will land.

### Tab Indicators

| Indicator | Meaning |
|-----------|---------|
| Grey dot (dim) | Idle — no session or command never ran |
| Blue pulsing dot | Busy — producing output now |
| Green dot | Done — command completed |
| Purple dot | Unseen — completed while you were viewing another tab (clears when selected) |
| Orange pulsing dot | Question — agent needs user input |
| Red pulsing dot | Error — API error or agent stuck |
| Question icon | Agent is asking a question |
| Progress bar | Operation in progress (OSC 9;4) |
| Amber gradient | Session created via HTTP/MCP (remote session) |

Final API failures keep the red Error dot even when the agent prints a completion summary or returns to idle. Optional auto-retry sends up to three `continue` commands (after 5, 15 and 30 seconds); an idle agent that does not resume stays red. Native provider retry countdowns do not count as final failures. Actual resumed work clears the error, and the next successful idle turn returns to Done.

Sidebar branch icons also show purple when they contain unseen terminals.
The mobile session list and session header use the same status colors: blue for
working, green for idle, orange for input, and purple for a completion not yet
opened on that phone. Opening the session clears its purple status.

Agent activity combines native lifecycle hooks, terminal movement (text
changing above the input area means the agent is active), and the visible
ready prompt. Claude, Codex, Gemini, Grok, pi, and OpenCode require a stable ready
screen before safety-sensitive actions such as auto-standby or queued agent
message delivery. Pressing Ctrl-C or Escape requests interruption but does not
turn the dot green until the agent confirms the interruption, returns to its
prompt, or exits.

A newly launched detected agent remains in its starting state until terminal
activity is actually observed. Question and answer transitions are retained by
the backend even if a browser or event-stream client temporarily falls behind.

Current Claude and Codex status lines are also recognized when their interface
keeps an empty composer visible or freezes during a long tool. Completed timing
summaries are not treated as work, so the indicator can still return to idle.
If a mail wake reaches Claude while its detailed transcript hides the composer
and Claude produces no response, the indicator returns to idle after five minutes
without output, provided the last lifecycle hook reported idle.
Grok keeps its composer visible while responding, so TUICommander waits for the
animated status row to disappear before treating that composer as ready.
Queued notices to Claude and Codex may take several seconds to confirm while
their hooks or internal prompt queues run. If TUIC reports that agent input was
not confirmed, inspect the transcript and composer before pressing Enter again.

A ready prompt means the terminal can accept input; it does not necessarily
mean the agent's turn is finished. If the agent still owns a background command,
the activity indicator remains working, parent-agent idle/completed notifications
are deferred, and auto-standby will not pause the session. Persistent integration
helpers are ignored, so they do not keep a completed turn active indefinitely.

### Tab Shortcuts

Hover a tab to see its shortcut badge: "Terminal N (Cmd+N)". Use `Cmd+1` through `Cmd+9` to jump directly. `Ctrl+Tab` / `Ctrl+Shift+Tab` — Next / previous tab.

### Scroll Shortcuts

| Shortcut | Action |
|---|---|
| `Cmd+Home` | Scroll to top |
| `Cmd+End` | Scroll to bottom |
| `Shift+PageUp` | Scroll one page up |
| `Shift+PageDown` | Scroll one page down |

### Selecting text during output

Selections remain attached to the same retained text while new output arrives,
including when the scrollback buffer reaches its limit. Once selected rows are
evicted, the selection is cleared instead of moving onto their replacements.
Resizing or switching between the primary and alternate screen also clears the
selection because the row layout changes.

### Scrollback in fullscreen apps

Agents launched through TUICommander, including supported agents typed into a new TUIC shell, use native scrollback where their CLI supports it (see [AI Agents](ai-agents.md#native-scrollback-on-launch)). This keeps past conversation lines available after the agent exits. The separate alternate-screen history below remains for programs that use fullscreen mode, such as `vim` and `less`, and for agents that enter it despite their launch defaults.

Apps that take over the screen (`gh run watch`, `less`, `man`, TUIs) run on the terminal's *alternate screen*, which by the original terminal spec has no scrollback at all — anything printed past the bottom of the window is gone. TUICommander enables an isolated alternate-screen history, so the scrollbar stays available and you can reach what rolled off the top.

Two details worth knowing:

- The scrollback of a fullscreen app is **wiped when it exits**, and never mixes with your shell's history.
- TUICommander preserves the application's actual output. A live view that reprints a frame taller than the viewport can therefore leave repeated snapshots in history; they are not deduplicated.
- Apps with mouse support (`vim`, `htop`, `lazygit`) receive the wheel themselves — use **Shift+wheel** or drag the scrollbar to scroll TUICommander's history instead.

## Zoom

Per-terminal font size control:

| Action | Shortcut | Effect |
|--------|----------|--------|
| Zoom in | `Cmd+=` | +2px font size |
| Zoom out | `Cmd+-` | -2px font size |
| Reset | `Cmd+0` | Back to default size |

Range: 8px to 32px. Each terminal has its own zoom level, shared by its CLI and Chat views. Zoom shortcuts work in either view; the all-terminals variants update both views too. The current zoom is shown in the status bar.

## Split Panes

Split the terminal area into two panes:

### Creating Splits

- **Cmd+\\** — Split vertically (side by side)
- **Cmd+Alt+\\** — Split horizontally (stacked)

The new pane opens a fresh terminal in the same working directory. Maximum 2 panes at a time.

### Navigating Split Panes

- **Alt+←/→** — Switch between vertical panes
- **Alt+↑/↓** — Switch between horizontal panes
- The active pane receives keyboard input

### Resizing Split Panes

Drag the divider between the two panes to adjust the split ratio. Both terminals re-fit automatically when you release.

### Maximizing a Split Pane

- **Cmd+Shift+Enter** — Maximize / restore active pane (zoom pane). Expands the focused pane to fill the full terminal area; press again to restore the split.

### Closing Split Panes

- **Cmd+W** closes the active pane and collapses back to a single pane
- The surviving pane automatically receives focus

### Split Layout Persistence

Split layouts are stored per branch. When you switch branches and come back, your split configuration is restored.

## Detachable Tabs

Float any terminal into its own OS window:

1. **Right-click** a tab → **Detach to Window**
2. The terminal opens in an independent floating window
3. The PTY session stays alive — the floating window reconnects to the same session

When you close the floating window, the tab automatically returns to the main window.

**Requirements:** The tab must have an active PTY session. Tabs without a session (e.g., just created but not connected) cannot be detached.

## Find in Terminal

Search within terminal output with `Cmd+F`:

1. Press `Cmd+F` — a search overlay appears at the top of the active terminal pane
2. Type your search query — matches highlight as you type (yellow for all matches, orange for active match)
3. Navigate matches:
   - `Enter` or `Cmd+G` — Next match
   - `Shift+Enter` or `Cmd+Shift+G` — Previous match
4. Toggle search options: **Case sensitive**, **Whole word**, **Regex**
5. Match counter shows "N of M" results
6. Press `Escape` to close the search and refocus the terminal

In **CLI**, search is integrated directly with the terminal grid for accurate match highlighting. In **Chat**, **Cmd/Ctrl+F** opens the transcript search: enter text and press **Enter** / **Shift+Enter**, or use **Next** / **Previous**, to select and scroll to matches. Matches span inline Markdown formatting within a paragraph; collapsed thinking bodies are skipped until opened. **Escape** closes search, including after clicking the navigation buttons. Switching CLI/Chat closes the search and clears its highlight.

## Cross-Terminal Search

Search text across all open terminal buffers from the command palette:

1. Press `Cmd+P` and type `~` followed by your search query (e.g. `~error`)
2. Results show terminal name, line number, and highlighted match text
3. Press `Enter` or click a result to switch to that terminal and scroll to the matched line (centered in viewport)
4. Minimum 3 characters after the `~` prefix

Also accessible via the "Search Terminals" command in the palette.

## Copy & Paste

- **Copy:** Select text in the terminal, then `Cmd+C`. A "Copied to clipboard" confirmation appears in the status bar. Multi-line Claude messages paste as clean text: the repeated `▎` visual gutter is removed while bullets, numbering, and indentation are preserved. Inside such a quote, rows that Claude broke only to fit the terminal width are joined back into one paragraph, so pasting into Slack or an email keeps whole sentences. Blank rows, list items and deeper indents keep their own line, and a quote that never reaches the terminal edge is copied exactly as shown.
  Claude prompt selections also remove the first `❯ ` marker and the two-column continuation margin. Width-supported wraps join, while short typed lines and additional content indentation remain. A glyph pasted inside the prompt remains part of the text. Composer cleanup applies only when the selection starts at column zero of the first composer row. Selecting body text or a VT soft-wrap continuation preserves literal markers and indentation.
- **Paste:** `Cmd+V` writes clipboard content to the active terminal. Text takes priority when a copied Chat selection or other rich content also includes an image. Screenshot-only pastes and copied image filenames from Finder still send Ctrl+V so terminal agents can attach the image.

### Copy on Select

When enabled (Settings > Terminal > Copy on select), selecting text in the terminal automatically copies it to the clipboard. A brief "Copied to clipboard" confirmation appears in the status bar. This is enabled by default.

## Clear Terminal

`Cmd+L` clears the terminal display. Running processes are unaffected.

## Terminal Bell

The bell behavior when receiving BEL character (\x07) is set by `bell_style` in `config.json` (Settings has no control for it):
- **none** — silent
- **visual** — brief screen flash
- **sound** — plays notification sound
- **both** — flash and sound

## Clickable File Paths

File paths appearing in terminal output are automatically detected and become clickable links. Hover over a path to see the link underline, then click to open it.

- `.md` / `.mdx` files open in the Markdown viewer panel
- Other source files open in the built-in editor
- A `:line` or `:line:col` suffix opens the built-in editor at that position, including for Markdown files. Clicking another position in an already-open file moves its cursor without replacing unsaved edits

Paths are validated against the filesystem before becoming clickable — existing files and directories show as links. Directory links open the File Browser at that directory; file links open the usual viewer or editor. Tilde paths expand to the home directory, and relative paths resolve from the terminal working directory. A path removed before opening shows a short notification instead of an empty tab.
Absolute paths can point outside a registered repository, including files in hidden directories.

Recognized extensions include: `.rs`, `.ts`, `.tsx`, `.js`, `.jsx`, `.py`, `.go`, `.java`, `.css`, `.html`, `.json`, `.yaml`, `.toml`, `.sql`, and many more.

## Plan File Detection

When an AI agent emits a plan file path (e.g., `PLAN.md`), a button appears in the toolbar showing the file name. Click it to open the plan — Markdown files open in the viewer panel, others open in the IDE. Click the dismiss button (x) to hide it.

## Working with AI Agents

When TUICommander sends text followed by Enter to an agent, it leaves a short
pause so terminal-based agent interfaces can process the text first. An agent
whose type is still unknown uses the longer Codex-safe pause.

TUICommander detects rate limits, prompts, and status messages from AI agents:

- **Rate limit detection** — Recognizes rate limit messages from Claude, Gemini, OpenCode, Codex
- **Prompt interception** — Detects when agents ask yes/no questions or multiple choice
- **Status tracking** — Parses token usage and timing from agent output
- **Progress indicators** — Shows progress bars for long-running operations

When an agent asks a question, the tab indicator changes and a notification sound plays (if enabled).
On the mobile PWA, a waiting Codex `request_user_input` question adds a control to the session header. Tap it to open Codex's question panel. The question and numbered options appear above the composer; tap an option once to answer. Tap **Other** to enter a typed note. The panel overlays the terminal without permanently reducing its height. The Rust parser that supplies those options takes effect after TUICommander restarts.
For a Claude AskUserQuestion dialog, the mobile session shows the title and
all choices and presents those choices above the composer. Tap a choice to
navigate to it and submit it; the app uses Claude's arrow-and-Enter contract.
If a Claude picker appears only in the CLI output, use the key bar's Up/Down
and Tab buttons to navigate it, then Enter to select. Navigation does not send
or replace a typed answer in the mobile composer.
Claude `/resume` also keeps its session list and search results visible in CLI
view. Use Up/Down and Enter to select a session, or Esc to close the picker.
The backend update requires a TUICommander restart before it reaches an
already-running development instance.
Remote HTTP/MCP workers respect **Silence orchestration completions** even after
a frontend reload, and a single busy cycle produces at most one completion
notification when idle and process exit arrive separately.
Generic desktop notifications such as “needs your attention” do not by
themselves mark the tab as awaiting input; TUICommander requires explicit
permission, approval, or waiting-for-input wording, an agent hook, or a verified
question on screen.
Questions inside a wrapped suggested follow-up action do not mark the tab as awaiting input.
On mobile, a blocked report can keep its **Input** state and still offer suggested follow-up actions. These actions replace generic Yes/No buttons. Native dialog choices take priority over suggestions; without either, generic confirmations remain available.
Canceling a Codex approval with Esc clears its question indicator when Codex returns to its composer.

### Queueing Follow-up Commands

In Compose, paste an image, drop PNG, JPEG, WebP or GIF files on the editor, or use **Attach images** beside Send to choose several. Drops insert at the pointer; picked files replace the current selection in their chosen order. Each saved image appears as `[image: <path>]`. Other file types and failed saves show an error without inserting a tag.

The Compose panel can leave work for an agent without steering its current turn:

- **Ctrl+Enter** sends immediately.
- **Shift+Ctrl+Enter** or the queue button submits immediately when the agent is
  already idle; otherwise it waits for the next idle window.
- Queued user commands and peer messages share one FIFO and are delivered one
  item per idle window in acceptance order.
- If an idle agent reports blocked progress, your queued command answers that
  wait. Native questions and permission dialogs still require an explicit answer
  before queued work can be delivered.
- The `N queued` badge counts only Compose commands. Clicking it discards those
  commands but never clears peer or orchestrator messages waiting in the same
  delivery queue.

Queueing is available only for detected agent sessions, not plain shells.

After a queued command reaches Codex, TUICommander allows several seconds for a stop hook to finish before deciding whether the agent accepted it. If no Working screen appears, check the transcript and composer before pressing Enter again; TUICommander will not replay an uncertain command automatically.

### Submitting through MCP

Automation that must start a new managed-agent turn uses one
`session action=submit session_id=<id> input=<command>` call. TUICommander
refuses busy agents, interactive dialogs, partial drafts, and sessions with
older queued work instead of steering or overwriting them. The same response
reports whether the complete PTY write occurred and whether the child terminal
moved after Enter; no follow-up polling call is required. Terminal movement
does not mean the agent understood the command or completed the work. If a
complete write times out without movement, do not replay it automatically.

`session action=input` remains available for raw text, prefilling, and
interactive keys. Its `ok:true` reports only that bytes were written.

### Session Restore

On restart, only terminals that had an active agent session are restored — plain shell tabs are discarded and a fresh terminal is spawned. For restored agent tabs, a clickable banner appears: "Agent session was active — click to resume." Clicking the banner sends the agent's resume command. Press **Escape** or click the **x** button to dismiss the banner without resuming.

### OSC 8 Hyperlinks

Terminal output that uses the OSC 8 standard for hyperlinks (e.g., URLs emitted by `ls --hyperlink`) is supported. Clicking an OSC 8 hyperlink opens the URL in your system browser.

### Answers-only View

Use **Toggle answers-only view** (`Cmd+Alt+R` on macOS) to read selectable marked answers and their tracked prompts. Turns without marked answers are omitted. Output before the first tracked prompt remains available as a prompt-less turn, from the retained history base. If no answers qualify, the view shows a one-line notice. Toggle the view again to return to the terminal.

### Stored terminal marker coordinates

When old output leaves the scrollback, command boundaries and prompt ticks stay attached to their retained output. Ticks for discarded prompts disappear; answers-only history keeps the retained question and answer association.

Terminal stream reconnect notices use the current terminal name when available and show the transport’s current attempt (up to 10). A received grid frame or confirmed empty replay removes only that stream notice; socket opening alone does not. Exhausted retries leave a persistent failure notice. Closing the terminal removes its notice, and late subscription failures cannot publish a notice after closure.

### Open a terminal link in Design Mode

Right-click an HTTP or HTTPS link, including a localhost dev server, then select **Start in Design Mode** and choose **Edge** or **Chrome**. The dedicated browser opens that URL and connects the agent in that terminal, not the focused tab. File and other non-web links retain Open and Copy link only. Browser extension setup is required once per repository profile.

### History-anchored viewport scrolling

Scrolling into history keeps the requested lines in view while an agent writes more output. If the scrollback limit removes those lines, the view stops at the oldest retained line. Scroll to the bottom to follow output again.

### Desktop dictation in Chat

Dictation inserts into Compose when focus is on the Chat transcript or page background. It replaces the selection that existed when you pressed the hotkey and keeps the rest of the draft. A focused search or form editor still receives dictation. In CLI, dictation keeps the usual terminal and auto-send behavior.

Changing focus or tabs while transcription finishes does not change the destination. If the original editor or session closes, or its draft changes, recover the text from the sticky error message or Messages bell. The app does not send it to another terminal.


### Waking a suspended tab

Open a suspended tab to resume it. A submitted command or peer message can also wake it without the desktop UI. Commands wait until the agent is ready; messages stay in its inbox. The stable peer address does not change. If resume fails, the tab stays suspended and the caller receives the reason.

### Mobile Claude Chat

Select **Chat** in the session header to read the shared conversation view.
The backend decides whether the Claude transcript is available, including when
the desktop has not discovered a session id yet. An unavailable transcript returns
the view to CLI with a notice. The mobile composer stays available in both modes.
Switch to CLI for questions missing from Chat; CLI returns to the latest output.

### Automatic idle-agent hibernation

The backend checks unattended top-level agent tabs every five seconds. `agents.hibernate_idle_minutes` defaults to 60 awake minutes; zero disables the sweep. `agents.hibernate_scope` defaults to `background_only`; `always` also permits foreground hibernation. Both machine-owned controls are in Settings → Agents (Expert Mode).

Hibernation requires an idle agent and a verified resume session in its saved profile. Plain shells and agents without a verifiable resume session stay live. Unread peer mail, pending commands or background work, awaiting input, attached readers, keep-open tabs and open automation runs prevent hibernation. Managed children retain their separate idle-close policy. Sleep gaps longer than 30 seconds do not advance the idle clock.

The normal Rust suspension path persists `reason: "idle_hibernate"` before ending the PTY. Opening the tab, submitting input or sending peer mail uses the shared wake path. Storage or resume-verification failures leave the live session intact.

### ego provider recovery

When terminal ego reports a provider outage, its tab shows **recovering** rather
than a working animation. The status bar shows the recovery phase, attempt,
next attempt time and seconds remaining. Zero seconds means the scheduled time
has arrived, not that the provider has recovered. **Force retry** submits
`/retry` once to that terminal. If the composer contains a draft, the retry is
refused without changing the draft. The status bar displays the refusal. Managed parents receive recovery entry and exit
notices; MCP callers can use `session action=force_retry session_id=<id>`.
Recovery clears when ego explicitly clears it, resumes busy/idle state, or
returns to the shell prompt. This terminal protocol is separate from ACP retry.

### Copying marked answers

Terminal selection copy, AI Chat answer buttons (desktop and mobile), and the Answers view omit the leading `💬` protocol marker. List prefixes and emoji inside prose or fenced examples stay in the copied text.

### Spaces at terminal wrap points

Mobile CLI output and session text reads preserve spaces when text wraps at the terminal margin. A word split across two terminal rows remains one word. Explicit line breaks remain separate. The mobile client uses exact wrap information from current backends; older backends retain the previous text reflow behavior. Chat reads the agent transcript separately and does not rebuild text from terminal rows.

On mobile, scroll back in CLI or Chat to show the floating down-arrow at the bottom-right of output, above the key bar. Tap it to return to the latest output without sending terminal input. The button hides at the bottom and stays outside the composer.

### Abandoned shells

Desktop sidebar session rows and mobile session cards mark a parentless plain shell as **Abandoned** after one idle hour and show its idle age. The five-second backend sweep requires a fresh OS process inventory: the PTY child must be an interactive shell with no child processes. An unknown inventory, script, foreground command, background child, detached TUIC job, managed identity, live child session, pending input or keep-open hold prevents closure. Unsupported or incomplete OS process observations fail closed.

Settings → Agents → Idle agent hibernation (Expert Mode) includes **Close abandoned shells after (hours)** (`agents.abandoned_shell_hours`). The machine-owned value defaults to **0 (off)**; whole hours from 1 to 65535 enable closure. Idle age is measured from the most recent input, output or semantic activity, including elapsed time while the machine sleeps. Closure rechecks the process inventory and activity at the destructive boundary, and uses the normal session close/tombstone path. The sweep can lag the threshold by one five-second tick. A remote daemon applies its own setting, not the desktop host's value.
