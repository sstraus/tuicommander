# Settings

Repository Settings separates identity, inherited worktree overrides, Design Mode, scripts, team sharing, and reset. Nullable controls retain their global-default choice. Reset clears override fields, including scripts, but retains repository identity and its Design Mode URL. Team exports never include scripts. The Dev Server URL and setup, run, and archive script fields have contextual help; hover or focus a help badge to read it, or click to keep it open.

Agents Settings shows the selected machine in the header. Run configuration editors keep explicit Save and Cancel actions. Terminal-agent permissions and filesystem sandbox remain separate from AI Chat ACP settings; launch instructions still save on blur.

Workspace pages use the same cards and aligned rows. Terminal retains its live canvas preview and all clipboard and scrollback controls. Keyboard Shortcuts places its searchable actions in Shortcut groups, before their keycaps, with explicit Rebind and Reset actions. Git & GitHub separates authentication, monitoring, repository defaults, worktree defaults, additional accounts, CircleCI, and repository bindings. Existing save boundaries and policy choices are unchanged.

The Application pages use section cards and labelled rows. General places the update status and its check action above the page header. On desktop, a Local tools card groups TUIC CLI, Code Intelligence, and Custom Launchers; this card is hidden in browser mode. General links to AI Chat defaults when AI Chat is enabled. The existing install, update, uninstall, and launcher actions keep their save and confirmation behavior. Appearance keeps group rename validation and client-local chat preferences. Notifications keeps visual controls available without audio; output-device discovery starts only after **Choose output device**. Changing master volume previews once on release.


The settings-service policy foundation keeps secrets write-only. Changes that
expose TUIC or select executable code require the existing confirmation dialog;
ordinary preferences, push settings and outbound HTTP destinations do not. The
dialog uses a backend-built diff and approvals expire after five minutes and
cannot be reused. The replacement settings MCP/API transport is still awaiting
its separate cutover (#1786-9cbd).

Open settings with `Cmd+,`. Settings are organized into pages. The navigation
groups the pages by task:

| Group | Pages |
|-------|-------|
| **Application** | General, Appearance, Notifications |
| **Workspace** | Terminal, Keyboard Shortcuts, Git & GitHub |
| **AI** | Agents, AI Chat, Voice, Smart Prompts |
| **Integrations** | MCP, Remote Access, Remote Machines, Telegram, Manage plugins |
| **Plugin settings** | Loaded plugins with registered settings, marked **Plugin** |
| **Repositories** | One page for each repository in the sidebar |

The **AI Chat** page is shown only while **Experimental Features** is on (see
[AI Chat](#ai-chat)).

Settings uses short labels and behavior descriptions. Implementation ownership and persistence details stay in the documentation rather than page introductions. Repeated scope chips and permanent usage walkthroughs are omitted. Catalogue labels have space above their controls, and header scope badges stay within the page width. Catalogue actions use a wrapping row with 8px gaps and 8px clearance below the field separator; the field effect appears once beneath its name, while the status line reports only the value source.

On narrow screens, the page selector truncates long names before its arrow. Settings reserves the same scrollbar space on every page, including plugin loading and error states. Help badges follow the final line of wrapped labels, and repository color actions keep the shared right alignment.

Plugin pages show their source plugin ID below the page title and in the navigation tooltip. The Settings header, navigation and footer stay visible while plugin values load. Loading and read errors appear only inside the native editor; switching pages does not load a plugin iframe. Unloading a plugin removes its page and search entries.

## Search

A search box sits at the top of the page list. Typing filters settings across
every page at once, so you do not have to know which page owns the one you want.
Each result shows the setting name and the `Page › Section` trail it lives under;
selecting one opens that page and scrolls to the field.

Search reads the settings catalogue without opening every page or running their
service probes. Plugin search uses the loaded plugin's described fields; values
are read only when you open its editor. Reopening Settings clears the previous
query so a requested page or section is visible.

The following search rules are deliberate:

- **Repository pages are not searched.** A repository page belongs to one specific
  repository, and a global box has no way to know which one you mean.
- **Per-agent controls in expandable cards are not searched.** Search cannot
  select a specific agent card or scroll to a control inside it. For managed
  workspace trust, open **Settings → Agents**, expand **Claude** or **Codex**,
  then use **Accept workspace trust for managed spawns** in Expert Mode.
- **ego permissions are searchable.** Permissions and Filesystem sandbox are
  in a visible section on the Agents page; no agent card needs expansion.
- **Settings the current build does not render are not listed.** The same rule
  hides the AI Chat page while Experimental Features is off, so a search does
  not open a page that is not in the navigation.

A result for an expert setting shows an **Expert** badge. See the next section.

## Expert Mode

Some settings have a default that is correct for almost everyone. These are
*expert* settings. The **Expert** switch in the Settings header controls them:

- **Expert off** (basic mode) — an expert setting is hidden while it has its
  default value. When its value is different from the default, it is shown, so
  an override is never hidden from you.
- **Expert on** — all expert settings are shown.

The switch position is kept between restarts (UI preference
`settings_expert_mode`, see [UI Preferences](../backend/config.md#ui-preferences-ui-prefsjson)).

A setting that you change stays shown until you open Settings again. If you
set it back to the default, it does not disappear under the cursor. The next
time you open Settings, it is hidden again if it has its default value.

When you select a search result with the **Expert** badge in basic mode,
Settings opens the page, shows that setting and scrolls to it. The setting stays
shown until you open Settings again. The switch does not change.

If TUICommander cannot read the default values (the `get_config_defaults`
command), all expert settings stay visible.

These settings are expert. All other settings are always shown. A page is
never all expert: in basic mode it would show an empty page.

| Page | Expert settings |
|------|-----------------|
| General | Auto-Standby Timeout, Content Indexing, Update Channel |
| Notifications | Master Volume, Audio Output Device |
| Terminal | Shell, Font Weight, Allow OSC 52 clipboard writes, Block folding, Show scrollbar marks, Reflow scrollback on resize |
| Git & GitHub | Auto-Delete on PR Close, Copy ignored files, Copy untracked files, Storage Strategy, Auto-archive merged worktrees, Orphan Worktree Cleanup and safe cleanup countdown, After Merge Behavior, Auto-Fetch Interval, the **Add another GitHub account** button (shown while no additional account exists) |
| Agents | Collect project progress, Launch instructions (global); per agent: Launch instructions, Close idle managed child after, Auto-retry on server errors, Prevent alternate screen, Accept workspace trust for managed spawns (Claude and Codex), Native status signals, Install hooks globally, Track agent intent, Collect progress, Show suggested follow-ups, Headless Command Template; Claude only: Environment Flags |
| Voice | Long-press threshold, Auto-send, Input device, Level gate, Speech confidence gate, Hold-back before sending, Notify model when hands-free changes, Start notice |
| MCP | Collapse tools |
| Remote Access | Port, Session Token Duration, Enable IPv6 (dual-stack) |

## Application

### General

| Setting | Description |
|---------|-------------|
| **Language** | UI language. The list offers the locales that ship with a message catalog, each named in its own language, and the pick applies immediately — no reload. A locale without a catalog is never offered, because it would render English while claiming to be translated. The picker is hidden while only one catalog ships — today that is English alone — because a list of one is not a choice. |
| **Show agent context bar** | Show the model's current intent, its orchestrator-assigned task, and the last prompt sent to an agent. |
| **When closing the main window** | Desktop: Ask (default), Background, or Quit. Ask offers Remember. Background hides visible windows and keeps sessions running. Use the application menu to quit; on macOS, click the Dock icon to show the window again. |
| **Hide Dock icon in background** | On | macOS desktop only. Hide the Dock icon while windows are hidden; restore it on Show. |
| **Confirm before quitting** | Confirm real quit with active terminals, including Cmd/Ctrl+Q. Background does not prompt. Detached-panel close closes only that panel. |
| **Confirm before closing a tab** | Ask before closing terminal tab |
| **Prevent sleep when busy** | Keep the machine awake while agents are working (**Power Management** section) |
| **Auto-Standby Timeout** | Pause idle background sessions after this duration to save resources. Default 5 min; `0` disables it. |
| **Content Indexing** | When to build search indexes: Disabled, Active repo only, Active + on switch, or All repos at boot |
| **Automatically check for updates** | Check for new versions on startup |
| **Update Channel** | Stable, Beta or Nightly. Beta checks the next beta and final stable; beta downloads open their release page. New beta installs default to Beta. Older beta installs with Stable saved move to Beta once; selecting Stable afterwards keeps that choice |
| **Wait for agents before restarting** | After an update installs, restart only when no session is busy or awaiting input. The APP UPDATE notification shows the busy count and offers Restart now. Default on |
| **Resume all agents after an update** | After the app restarts for an update, resume every agent tab with a confirmed session on every branch. Default on |
| **TUIC CLI** | Install or uninstall the `tuic` command-line tool, with its status. Desktop app only. See [CLI](cli.md). |
| **Code Intelligence** | Install and manage MDKB, which gives the editor go-to-definition, find references, and symbol outline, and serves as a memory manager for AI agents. Desktop app only. |
| **ego executable** | Path to the ego binary the **AI Chat** panel talks to over ACP (**ego** section, directly after Code Intelligence). Always shown, also while Experimental Features is off. On the desktop it is laid out like the TUIC CLI section: a status line ("Configured at …") with **Select…** (native file picker) and **Clear**. A browser client, which has no native picker, gets a text field. While it is empty, ACP is not configured: every connect is refused in Rust and the panel says so rather than launching nothing. Read at each connect, so a correction takes effect without a restart. |
| **ego profile** | Optional name of a profile in ego's user configuration for the AI Chat panel. Leave empty to use ego's normal profile selection. Names with whitespace or a leading dash are rejected. TUICommander passes the name to `ego acp`; the profile's rules remain in ego's configuration. |
| **Default IDE** | IDE for "Open in..." actions (**IDE** section). Only installed apps are offered, grouped by category: Code Editors (VS Code, Cursor, Zed, Windsurf, Neovim, Xcode, `$EDITOR`), JetBrains (IntelliJ IDEA, PyCharm, WebStorm, GoLand, CLion, PhpStorm, RubyMine, Rider, DataGrip, RustRover, Android Studio, Fleet), Terminals (Ghostty, WezTerm, Alacritty, Kitty, Warp, iTerm2), Git Tools (Sourcetree, GitHub Desktop, Fork, GitKraken, Sublime Merge, Tower), System (Terminal, Finder) |
| **Custom Launchers** | Define your own tools for the "Open in" menu. Each launcher has a name, an executable (bare name resolved on `PATH`, or absolute path), and arguments (one per line). Arguments may use placeholders, expanded at launch: `{path}`/`{file}` (focused file, else repo root), `{fileDir}` (directory of the focused file), `{repo}` (repo/worktree root), `{cwd}` (focused terminal's working directory), `{home}` (your home directory), `{line}`/`{column}` (1-based editor cursor position). Args are passed verbatim (no shell parsing), so paths with spaces are safe. |
| **Experimental Features** | One toggle, no sub-toggles. It opts in to the **AI Chat** panel (ego over ACP, #785-58ca) and **SSH Tunnels**. It also shows the **AI Chat** settings page. The AI Chat, AI Triage and AI Watchers sub-toggles went with the embedded AI engine (#784-0aec). |

### Appearance

| Setting | Default | Description |
|---------|---------|-------------|
| **Split Tab Mode** | — | Separate or unified tab appearance |
| **Tab Ordering** | — | How tabs are ordered: grouped by type, terminals first, or freely interleaved |
| **Cycle All Tab Types** | Off | When on, next/prev-tab shortcuts also cycle file/diff/markdown/editor tabs (ordered like the tab bar). Off cycles terminals only. |
| **Nested Terminal Tabs** | Off | Opt in to collapsible branch activity cards. When off, the sidebar shows no activity caret or nested agent/session rows. When on, branches with open sessions show agent status, current activity, and last-update age. Applies immediately. |
| **Max Tab Name Length** | — | 10–60 slider |
| **New text indicator** | On | Show **New text below** when terminal Chat or AI Chat receives content while you read earlier messages. Saved on this client; mobile Settings offers the same toggle. |
| **Repository Groups** | — | Create, rename, delete, and color-code groups |
| **Reset Panel Sizes** | — | Restore sidebar and panel widths to defaults (**Layout** section) |
| **Mute Intermediate Text** | On | **Chat** section. In the terminal Chat view, AI Chat and the mobile chat, assistant text that is followed by a tool call in the same turn is drawn smaller and dimmer, so status lines such as "Pump restart." are not mistaken for the answer. The last text of a turn keeps the normal style. Copy and Find work on muted text. Applies immediately. |
| **UI Legend** | — | Visual reference for colors, symbols, and badges used in the app |

The terminal and app color theme is on the [Terminal](#terminal) page. Native window and webview backgrounds use the same theme color at startup and when the theme changes, including light themes. The mobile PWA offers a separate Dark/Light choice in its Settings screen, saved on the server for connected mobile clients; the screen also shows app and server versions.

#### Show agents under branches

Nested agent rows are disabled by default. To show them, open **Settings → Appearance → Tabs** and turn on **Nested Terminal Tabs**. No restart is required.

After you enable the setting, a branch gets an activity caret when it has at least one open terminal session. Expand the branch to see its agents and shells. Each detected agent row shows its status, current intent or task, and last-update age; clicking a row switches to that session. The setting only displays existing sessions—it does not start or discover an agent outside a terminal assigned to that branch.

### Notifications

- When the desktop window is unfocused, agent questions and Progress `done`/`blocked` entries can appear in the operating system's Notification Center. TUICommander checks notification permission when the first alert is needed. On macOS, clicking an alert opens the named terminal or Progress project. An open, focused window receives no duplicate system alert.
- **Enable Audio Notifications** — Master toggle
- **Volume** — 0-100% (applied natively by the Rust playback path). Releasing the slider plays a short preview at the new level.
- **Audio Output Device** — defaults to the system output. Click **Choose output device…** to enumerate available outputs and pick a specific one. Enumeration is deferred until you click, because on macOS the audio device scan triggers the microphone-permission prompt — notifications never record audio.
- **Per-event toggles:**
  - Agent asks question
  - Error occurred
  - Task completed
  - Warning
  - Info
  - Attention (agent needs you)
- **Test buttons** — Test each sound individually. Normal notifications share one anti-spam interval across every sound type so a burst cannot play overlapping tones. The Test button bypasses that limit, so rapid A/B volume comparisons always play.
- **Silence subagent sounds and notifications** — On desktop and browser-desktop, mute all sounds and OS notifications from sessions spawned by another agent, including questions, errors, completion, attention requests and Progress updates. Tab state, Activity and bell entries still update; urgent user cards still show. Parent and user-started sessions are unaffected. This extends the existing orchestration control and keeps its default (on) and saved preference. Unparented HTTP/MCP sessions retain the completion-chime mute only. Each busy cycle can notify at most once, even when idle and process exit arrive separately.
- **Reset to Defaults** — Restore default notification settings
- **Keep toasts in the bell** — Retain user-action toasts in the toolbar bell's **MESSAGES** section, including their level and explicit action. Turning this off leaves ordinary user feedback transient; agent/MCP notices and agent spawns still go directly to the bell (identical consecutive notices within five seconds are deduplicated), and overflow from the two-card limit is retained there regardless of this setting. Dedicated-domain toasts that explicitly opt out of Messages remain visible or queued. Transient cards sit top-right of the terminal, clear of docked panels and bottom input; same-kind bursts within five seconds show ×N and restart their dismissal timer. Informational toasts leave after 20 seconds and warnings after 60 seconds; an error toast stays until you click it away (the bell keeps a copy). Errors take priority over informational cards, and tall cards scroll to keep actions reachable. This setting is visual and remains available without audio output.
- **Automatic agent toast cards** — MCP toast notices show a transient card only while their origin terminal tab is active. Settings → Notifications → **Show cards from the active terminal** defaults on and is saved per client; turning it off restores urgent-only cards. Urgent notices always show a card, and notices without an origin session remain bell-only unless urgent.

**Attention** is the distinct call-back-to-keyboard sound available to agent
toasts. Native playback and the browser fallback share a triangular G4→G4→E5
motif with two short knocks and a longer rise; each engine applies its own
envelope. It remains subject
to the master toggle, configured volume, and its own per-event toggle; native
playback also uses the selected output device.

## Workspace

### Terminal

| Setting | Default | Description |
|---------|---------|-------------|
| **Terminal Theme** | — | Color theme for terminal output and app chrome, with preview swatches (**Theme** section) |
| **Shell** | — | Custom shell path (e.g., `/bin/zsh`, `/usr/local/bin/fish`). Leave empty for system default. |
| **Terminal Font** | JetBrains Mono | 13 bundled monospace fonts: Fira Code, Hack, Cascadia Code, Source Code Pro, IBM Plex Mono, Inconsolata, Ubuntu Mono, Anonymous Pro, Roboto Mono, Space Mono, Monaspace Neon, Geist Mono |
| **Default Font Size** | — | 8–32px slider. Applies to new terminals; existing terminals keep their zoom level. |
| **Font Weight** | — | Terminal font weight (200 = ExtraLight, 400 = Regular, 700 = Bold) |
| **Cursor Style** | — | Bar, Block, or Underline |
| **Copy on select** | On | Auto-copy terminal selection to clipboard. When text is selected in the terminal, it is immediately copied. A "Copied to clipboard" confirmation appears in the status bar. |
| **Allow OSC 52 clipboard writes** | On | Let terminal programs set the system clipboard via the OSC 52 escape sequence (used by tmux, vim, ssh yank-over-SSH, etc.). Because OSC 52 is honored from anywhere in the byte stream, a displayed file or log can also overwrite the clipboard — so a non-blocking "Clipboard updated" notice appears on every write. Disable to ignore OSC 52 entirely. |
| **Show block timestamps** | On | While Ctrl+Cmd is held, each command block is labelled at the right edge with how long ago it started. |
| **Block folding** | On | Let the Toggle Block Fold shortcut (Cmd/Ctrl+Shift+.) and its command-palette entry collapse a command block's output. Blocks already folded stay collapsed when this is off. |
| **Show scrollbar marks** | On | Mark each command's position on the terminal scrollbar, so a long scrollback shows where output began. Covers the **history** markers only — the blue/red block ticks and the green user-prompt ticks. Orange search-match ticks are not affected: they are the result of a search you just ran, not a display preference. |
| **Reflow scrollback on resize** | On | Re-wrap scrollback history when the terminal changes width, so output written at the old width stays readable after a side panel opens or closes. Turn it off to leave history lines as they were written and truncate them to the new width instead. The visible screen is never reflowed either way — cursor-addressed TUIs redraw themselves. A change applies to sessions already open. |

The terminal bell mode (`none`, `visual`, `sound`, `both`) has no control on
the dedicated Terminal page. Use All settings (Expert) to edit `app.bell_style` (see [Terminal Bell](terminals.md#terminal-bell)).

### Keyboard Shortcuts

Browse and rebind all app actions. The same editor is also in the **Help panel**
(Help > Keyboard Shortcuts).

- Every registered action is listed with its current keybinding
- Click the pencil icon next to any action and press a new key combination to rebind it
- Custom bindings are stored in `keybindings.json` in the platform config directory
- Auto-populated from the action registry — new actions appear automatically
- **Global Hotkey (Toggle Window)** — an OS-level shortcut to show or hide TUICommander from any application (desktop app only)
- **Plugin Commands** — commands that plugins register, which you can bind like any other action
- Most shortcuts are also listed in the native system menu bar (desktop app only — browser/PWA clients have no native menu)

See [Keyboard Shortcuts](keyboard-shortcuts.md) for the full reference and customization guide.

### Git & GitHub

GitHub authentication, pull request and issue display, and the global
repository and worktree defaults.

| Setting | Description |
|---------|-------------|
| **OAuth Login** | Device Flow login — click "Sign in with GitHub", enter code on github.com. Token stored in OS keyring. (**GitHub Authentication** section) |
| **Auth Status** | Shows current login, avatar, token source (OAuth/env/CLI), and available scopes |
| **Disconnect** | Clear all GitHub tokens (keyring + env cache). Falls back to next available source. |
| **Diagnostics** | Token source details, scope verification, API connectivity check |
| **Auto-show PR popover** | Automatically display PR details when switching branches. Only shows for OPEN pull requests — CLOSED PRs are hidden, and MERGED PRs fade after 5 minutes of user activity. (**Pull Requests** section) |
| **Hide Draft PRs / Hide Conflicting PRs / Hide CI Failing PRs** | Filter the pull requests the app shows |
| **Auto-Delete on PR Close** | Off (default), Ask, or Auto — controls branch cleanup when a PR is merged/closed |
| **Show issues / Issue Filter** | Which issues to show in the GitHub panel: Assigned (default), Created, Mentioned, All, or Disabled (**Issues** section) |
| **Repository Defaults** | Base branch, **Copy ignored files** and **Copy untracked files** (two separate toggles), setup/run/archive scripts applied to new repos |
| **Worktree Defaults** | Storage strategy, branch-name prompt, branch deletion, auto-archive, orphan cleanup, merge strategy, after-merge behavior, auto-fetch interval. See [Worktrees](worktrees.md). |
| **Additional GitHub Accounts** | Extra github.com or Enterprise logins. See [GitHub Integration](github-integration.md). |
| **CircleCI** | Store or remove a read-only CircleCI token for failed job logs. The saved token is never displayed again. |
| **Repository Bindings** | Which account each workspace repo resolves to |

Token priority: `GH_TOKEN` env → `GITHUB_TOKEN` env → OAuth keyring. The `gh` CLI login is not used.

## AI

### Agents

**Close idle managed child after** sets the time a finished orchestrator-spawned
agent stays open for follow-up. The default is 15 minutes; 0 disables automatic
closure. User-created sessions are unaffected. Unread mail and background work
keep a managed child open, and its parent receives an idle-timeout notice before
closure. A child can also be marked keep-open at spawn or through the agent or
session tool.

**Launch instructions** combines generated modules with user additions. Select Markers, Working with the user, Waiting and long work, Git and worktrees, Process safety, and Children. Children applies only to spawned children. Global switches are ceilings for the per-agent switches. **Turn all launch instructions off** suppresses the complete text, including additions. When mdkb is installed, a fixed "Code search" rule (search with MDKB first) follows the modules; it has no toggle and is absent without mdkb.

**Add instructions** and **Add instructions for <agent>** open empty user-addition fields, not copies of the generated text. TUIC appends global additions and then agent additions after enabled modules. Save on blur; clear a layer with **Clear global additions** or **Clear agent additions**. Empty adds nothing. `{VER}` becomes the TUIC version.

Read-only effective previews follow the selected machine and draft changes. While its configuration loads, or if it fails, previews stay unavailable and **Add instructions** is disabled; text from the previous machine is not reused. Legacy default copies migrate away without duplication; custom text remains an addition. See [AI Agents](ai-agents.md#launch-instructions-tuic-output-markers).

Each supported agent has an expandable row showing detection status, version, and MCP badge.

The default Codex run configuration includes
`--dangerously-bypass-approvals-and-sandbox` in its editable arguments.
The warning icon on a configuration identifies an active bypass. Remove that
argument through **Edit** to enable normal approvals and sandboxing for new
launches. The removal survives restart. Existing direct Codex defaults receive
the argument once during migration; wrapper configurations keep their own
arguments and the managed-launch wrapper warning.

| Setting | Description |
|---------|-------------|
| **Agent Detection** | Auto-detects running agents from terminal output patterns. Shows "Available" or "Not found" for each agent. |
| **Run Configurations** | Custom launch configs (binary path, args, model, prompt) per agent. Add, set default, edit, or delete configurations (Edit / Delete live under the `···` menu on each row). A config named **"review"** enables the Review button in the PR Detail Popover — its args are interpolated with `{pr_number}`, `{branch}`, `{base_branch}`, `{repo}`, `{pr_url}`. The agent's **default run config** also drives resume: launching / resuming the agent swaps the agent's default binary (e.g. `claude`) for `command` and appends `args` after the resume flag. |
| **MCP Integration** | Install/remove TUICommander as MCP server for supported agents. Shows install status with a dot indicator. |
| **Native status signals** | Claude and Codex only. Enabled by default; injects process-scoped status configuration at launch without changing global agent files. The **Signals: at launch** badge identifies this mode. |
| **Prevent alternate screen** | Enabled by default for every agent. TUIC applies a verified native-scrollback control when the installed CLI supports one. Turning it off leaves new launches without TUIC's screen control. Existing shells need to be reopened to receive the changed policy. |
| **Accept workspace trust for managed spawns** | Claude and Codex only. In Expert Mode, expand the agent's card to change whether agent-to-agent spawns skip that agent's workspace trust question in a new folder. |
| **Install hooks globally** | Gemini, Grok, and OpenCode only. Explicitly installs/removes sentinel-owned lifecycle hooks in the agent's global configuration. Off by default. |
| **Show agent intent as tab title** | When agents declare their current work phase, update the tab name with a short title |
| **Show suggested follow-up actions** | Display actionable suggestions from agents after completing a task |
| **Collect project progress** | Global switch for the Progress journal. On by default. Off removes the `progress` tool from every agent's tool list, stops the reporting obligation being sent, and records nothing — including the `intent:` markers TUICommander writes itself. |
| **Collect progress** (per agent) | Per-agent override, shown with the agent's MCP settings and disabled while the global switch is off. The effective value is *global AND (per-agent, default on)*, so an agent with no opinion follows the global switch. An agent that reports while its own override is off is answered `progress_tracking_disabled` rather than silently ignored. |
| **Claude Usage Dashboard** | (Claude Code only) Toggle under Features when the Claude row is expanded. Enables rate limit monitoring, session analytics, token usage charts, activity heatmap, and per-project breakdowns. Usage data appears in the status bar agent badge and in a dedicated dashboard tab. |

See [AI Agents](ai-agents.md) for details on agent detection, rate limits, and the usage dashboard.

### AI Chat

Shows the configuration of `ego`, the engine behind the AI Chat panel. Offered
only while **Experimental Features** (General page) is on, because that is what
offers the AI Chat panel — the one place ego is reachable from.

The ego binary itself is a TUICommander setting and is on the
[General](#general) page (**ego executable**), like MDKB.

The page shows which providers and models `ego` can use, and which model it
starts from. Everything here is ego's, read and written by running ego:

- **Perimeter** — edit the first root, its read-only or read-write access, extra
  read roots and extra writable roots (one absolute or `~/` path per line).
  These roots extend the AI Chat workspace; they do not replace it. The absent
  roots setting uses ego's inherited effective roots, including user roots for
  the selected profile, or read-write `~/Gits` by default when present. Saving all paths
  empty publishes explicit `roots=[]` for workspace-only roots. A read root
  does not revoke access from a containing writable root.
- **Network enabled** — persist ego's `network="on"` or `network="off"` setting.
  Root saves and network writes are separate operations. A network change keeps
  any unsaved root draft. **Reload from ego** replaces that draft.
- **Effective perimeter** — `ego config ls --effective --json` in the configured
  AI Chat workspace and ego profile. It shows canonical admitted roots, origin
  layers, sandbox, backend, network and warnings. The form edits that profile's
  user configuration (or user defaults if no profile is selected). Existing
  conversations keep their admitted perimeter. **Enforcement not checked** is
  ego's current capability evidence, not proof that the OS enforces these roots.
  The badge is scoped to the **exec sandbox** for spawned processes. A measured
  array gets **Enforced by OS** only when every selected guarantee is present;
  partial or empty measured sets get **Prompt only** with an explanation.
  `off` with online network never gets a vacuous OS badge. Unknown evidence
  stays unverified. The raw capability array, reason and probe reference remain
  in the preview. Root tool permissions also have their own policy checks. A failed write/readback retains the last successful preview; reload to
  check the stored state. An older ego without the effective-view command reports
  its CLI failure instead of showing an invented perimeter.
- **Default model** — a picker over every model ego knows, grouped by provider.
  Choosing one runs `ego config set model="<slug>"` and then re-reads, so what
  you see afterwards is what ego persisted, not what was sent. It survives a
  restart because ego holds it, not TUICommander. A model ego marked unavailable
  cannot be picked.
- **Refresh from providers** — asks ego to re-enumerate its sources
  (`ego models --refresh`). This is the only action anywhere in TUICommander
  that reaches a provider over the network, and it is ego that reaches it.
  Opening the page does not.
- **Providers** — one row per source ego knows, with how many of its models are
  usable and why the rest are not, in ego's own words.
- **Credential state** — read from `ego doctor`. *stored*, *expired* (ego renews
  it on its next run; you do not log in again), *no credential*, or *ego could
  not read its credential store* — the last is not the same as an empty store.

**No API key ever enters TUICommander.** None is stored, none reaches the OS
keyring, and no provider HTTP call is made from this process. **Login** next
to a provider opens a terminal tab that runs `ego auth login <provider>` and
closes Settings so the tab is in front. The browser, device-code or API-key
prompt is ego talking to you directly in that terminal; TUICommander types the
command and never sees the key. The tab stays open after ego exits so a refusal
stays readable; reopening the page shows the credential state `ego doctor` now
reports.

When ego is not configured the page says so and points to the *ego executable*
field in Settings → General instead of rendering an empty list. A path that is
set but cannot be started is reported as its own thing, and also points to
Settings → General. When an ego command fails, what ego
printed is shown verbatim — the command, its exit code, and its output.

The in-chat model switch is a different control: that one is a session option
and changes one conversation. This changes the default every new run starts
from.

#### The old provider registry is gone

The provider registry, its slots and the API keys it put in the OS keyring were
deleted with the embedded AI engine (#784-0aec).
`<config_dir>/providers.json` is no longer read or written, and a file left by
an older version is inert. This page is not its replacement — it edits ego's
configuration and stores nothing of its own.

Agent CLIs (Claude Code, Codex, …) are unaffected — they keep their own
credentials and are configured on the **Agents** page.

**Smart Prompts in API mode** used the old registry's headless slot. It now runs
one unattended ego turn instead (#787-ee50): the model is whatever this page's
default is, the prompt is sent once, and the answer goes to the prompt's output
target. Nothing streams — there is no panel open to stream to. With no ego
binary named, the mode refuses and says to name one and pick a model on this
page. Shell, inject and headless-CLI modes are unaffected.

The turn runs with **no TUICommander tools**, so it cannot open a terminal or
touch a repository, and any permission ego asks for is declined at once: nobody
is watching, and a question nobody answers is a turn that never ends. If a
prompt comes back empty because of that, the refusal is reported rather than the
empty answer.

### Voice

Dictation, speech recognition, spoken replies, and hands-free conversation.

**Spoken replies** defaults to on. Turn it off to keep hands-free dictation and receive text replies. The toggle is also in the mobile conversation header and mobile Settings. All controls save the same preference on the server device.
Hands-free conversation uses an accent **Start conversation** action and a distinct **Stop conversation** action. The state row shows a coloured indicator with **Running** or **Stopped**, alongside the backend phase when available.
The global dictation hotkey and this machine's input devices are desktop-only;
a browser client shows the rest of the page. See [Voice Dictation](dictation.md)
for full details.

### Smart Prompts

Manage the AI-powered actions surfaced in the toolbar, context menus, and command palette. Reachable from the nav or directly via "Manage Smart Prompts..." in the Smart Prompts drawer.

- **Headless Agent** — default agent for headless prompts; individual prompts can override it
- **Prompt list** — grouped by category, with enable/disable toggle and placement/mode badges
- **Editor** (click a row) — name, description, content with variable insertion, placement checkboxes, Execution Mode, inject target, Auto-execute, output target, system prompt, keyboard shortcut
- Built-in prompts show "Reset to Default" once overridden; custom prompts can be deleted

See [Smart Prompts](smart-prompts.md) for the full guide.

## Integrations

### MCP

#### HTTP API Server

Enable the HTTP API server for external tool integration:
- Serves the REST API and MCP protocol for AI agents and automation tools
- Local MCP connections use a Unix domain socket at `<config_dir>/mcp.sock` — no port configuration needed
- AI agents connect via the `tuic-bridge` sidecar (auto-installed on first launch for every supported agent that is installed on the machine — see [MCP bridge auto-install](../backend/config.md#mcp-bridge-auto-install))
- Shows server status (running/stopped) and active session count

#### TUIC Tools

Native tools exposed to AI agents via MCP. Each tool can be individually enabled or disabled to restrict what agents can access.

**Manual MCP configuration** (expandable) — shows the `tuic-bridge` binary path and a ready-to-paste JSON snippet for manually configuring MCP clients that aren't auto-installed. Click "Copy" to copy the snippet to clipboard.

**Collapse tools** (checkbox) — when enabled, replaces the full tool list sent to AI agents with 3 lazy-discovery meta-tools (`search_tools`, `get_tool_schema`, `call_tool`). Cuts the baseline MCP context cost the agent carries every turn. Measured 2026-09-13 against the running desktop instance with 190 tools connected: the full list is 154,117 bytes / 35,104 tokens, the collapsed list 2,810 bytes / 615 tokens — and the collapsed figure does not move with the number of upstream tools, because they are no longer in the list. The agent fetches schemas on demand via BM25-ranked search. (Tokenizer: tiktoken 0.14.0 `o200k_base`, a GPT tokenizer used as a proxy; Anthropic publishes no offline tokenizer. Method and full table: [`mcp-http.md`](../backend/mcp-http.md#measuring-the-surfaces).) Native semantics do not change: a managed command is still one `call_tool` request for `session action=submit`, and its bounded receipt comes back in that response. Default: off. Grok sessions receive this compact surface automatically for compatibility with Grok's tool-name parser, without changing the checkbox or other clients. Toggling emits `notifications/tools/list_changed`; compatible clients refresh automatically, while clients that ignore the notification may require a reconnect.

**Call audit** (MCP page) — **Record MCP call outcomes** keeps a body-free, in-memory log of recent agent tool calls. When on, the table lists time, operation, known target name and id (or a digest when unresolved), outcome (Replied / Error / Cancelled), response bytes and effect (Read-only / Completed / Failed / Not started / Unknown), with an outcome filter and a Refresh button; it loads when the page opens and on Refresh only. Effect tooltips explain the observation: Completed means the handler returned, not that queued external work finished or the client received the reply. Unknown is reserved for uncertain completion, such as cancellation, an interrupted handler or a lost upstream reply. Default: off in release builds, on in debug builds.

**Capabilities** — three visible sections follow the approved MCP domains: **Always listed** (sessions, peers with task outcomes, repositories, workflows with story/run/proposal/report/launch controls, automations, user interaction with Design Mode, secrets, settings and progress), **Rarely needed** (diagnostics and plugin reference), and **Conditional** (voice, Telegram and remote deployment). Member controls stay nested under their domain without a disclosure. Conditional rows report whether voice or Telegram is configured or a remote server is connected, independently of their saved switch. Unavailable features remain configurable. A parent restriction also denies its member actions without changing saved member denials. Progress still requires global Progress tracking, and Settings requires trusted UI opt-in. Saved tool denials migrate once without re-enabling their new actions. Unknown old names remain visible as migration diagnostics. See [Capability cutover](../backend/mcp-http.md#capability-cutover-and-client-migration) for the breaking tool-name changes and reconnect requirements.

#### Upstream MCP Servers

**Manage in Settings** in the MCP popup (**Cmd+Shift+I**) opens the MCP page scrolled to this block — the block sits below the fold, so a plain page switch would look like nothing happened.

OAuth upstreams show **Authorize** when consent is required. TUIC prepares the OAuth request, then displays a blocking in-app confirmation naming the authorization-server origin before opening the system browser. Cancelling that confirmation discards the pending request.

Proxy external MCP servers through TUICommander. Their tools appear prefixed as `{name}__{tool}`:
- Add upstream servers via HTTP (Streamable MCP) or stdio (process) transport
- HTTP authentication starts with a masked Bearer field. Choose OAuth 2.1 when the upstream supports it.
- Use **+** beside **Secret headers** to add multiple name/value rows, and **−** to remove a row. Every value is masked and stored in the OS credential vault. Saved rows show an empty secret field: leave it blank to keep the current value, or enter a replacement.
- Header names and credential references are saved in `mcp-upstreams.json`; values are never saved there. Invalid names, duplicate names and Authorization beside Bearer/OAuth are rejected. Routing, framing and MCP protocol headers are reserved.
- An upstream with Bearer, OAuth or secret headers cannot change to another URL origin (scheme, host or port). Add a new upstream with a different name for a different provider. Same-origin path edits retain the saved auth settings. Stored credentials are checked against their saved origin before requests. After upgrading, credentials saved without an origin binding must be entered again, or authorized again for OAuth; the connection error explains this.
- Credentials are sent only to the configured origin. Same-origin redirects work; a redirect to another scheme, host or port stops with an upstream error.
- Live status (connecting, ready, circuit open, failed) with tool count and call metrics
- Reconnect and remove controls per upstream
- Per-repo scoping: each repo can define an allowlist of active upstream servers via **Cmd+Shift+I** popup (or repo settings). Empty/null allowlist = all servers active

See [MCP Proxy](mcp-proxy.md) for the full guide.

### Remote Access

Enable HTTP/WebSocket access from other devices on your network: port,
credentials, network interface, session token duration, IPv6, LAN access
without authentication, **Tailscale HTTPS**, the connect QR code, and the
**Cloud Relay**. See [Remote Access](remote-access.md) for full setup guide.

### Remote Machines

Add, connect, and manage `tuic-remote` daemons on other machines over SSH or a
direct URL. **Auto-update remote daemons** is a per-connection option, off by
default. It updates on connect only when the daemon has no live PTY sessions.
See [Remote Access → Remote Connection Manager](remote-access.md#remote-connection-manager).

### Plugins

Install, manage, and browse plugins. See [Plugins](plugins.md) for the full guide.

- **Installed** — List all plugins with enable/disable toggle, logs viewer, uninstall
- **Browse** — Discover and install from the community registry
- **Check for plugin updates** — fetch the registry at startup and show available updates

## Repositories

Per-repository settings. Open them from the **Repositories** group in the
Settings navigation, or from the sidebar `⋯` → "Repo Settings".

### Worktree Tab

- **Display Name** — Custom name shown in sidebar
- **Base Branch** — Branch to create worktrees from (auto-detect, main, master, develop)
- **Copy ignored files** — Copy .gitignored files to new worktrees
- **Copy untracked files** — Copy untracked files to new worktrees

### Scripts Tab

- **Setup Script** — Runs once after worktree creation (e.g., `npm install`)
- **Run Script** — On-demand script launchable from toolbar with `Cmd+Shift+R`
- **Archive Script** — Runs before a worktree is archived or deleted; non-zero exit blocks the operation
- **Dev Server URL** — Optional address of this repository's development server. Design Mode opens it in a dedicated Chrome window; with no URL, Chrome opens `about:blank` so you can navigate manually. See [Design Mode](design-mode.md).

### Repo-Local Config (`.tuic.json`)

A `.tuic.json` file in the repository root provides team-shareable settings that override per-repo app settings and global defaults. The dedicated repository page reads this file. All settings (Expert) can edit its catalogue fields through the registered-repository owner, with a revision check; you can also edit it in your repository.

**Precedence:** `.tuic.json` > per-repo app settings > global defaults

Supported fields: `base_branch`, `copy_ignored_files`, `copy_untracked_files`, `setup_script`, `run_script`, `archive_script`, `worktree_storage`, `delete_branch_on_remove`, `auto_archive_merged`, `orphan_cleanup`, `pr_merge_strategy`, `after_merge`, `auto_delete_on_pr_close`.

User-specific settings (`promptOnCreate`, `autoFetchIntervalMinutes`, and the Dev Server URL) are intentionally excluded from `.tuic.json`.

Repository-local config reads are independent. If a repository is unavailable, its last successful values stay in use while other repositories refresh. A successful read that finds no `.tuic.json` clears the cached local values. A later successful read adopts the recovered repository's current values.

### Telegram

Settings → Telegram configures the current machine. Bot setup, Chat access, Agent registration and Recent messages have separate cards. Authorized chats use named list rows with a Remove action, and Reload status refreshes the polling summary. Telegram setup is available only in desktop Settings, not in mobile Settings. Use it on the host running the desktop app or `tuic-remote`. Both share one polling owner on that machine.

1. Paste the BotFather token into the password field and select **Save and check bot**. TUIC writes `bot.token` with owner-only permissions and checks `getMe`. Settings shows the bot username and whether a token is set; it never reads the token back.
2. Enable Telegram. Ask an agent in the polling app to opt in with the Telegram MCP tool (`register`/`unregister`); Settings shows the registered agent read-only, or **No agent registered**. When the status is connected, select **Link chat**, then send the displayed six-character code to the bot within ten minutes. The code works once. Alternatively, type a positive private chat ID and select **Add chat ID**. Remove revokes authorization. The bot confirms a successful link with **Chat linked.**, and the consumed code hint disappears on the next Settings refresh. A bare `/start` explains how to get a code but does not authorize a chat. Wrong or expired six-character hexadecimal codes receive a safe retry instruction; other unauthorized text receives no reply. With no registered agent, an authorized message reaches the latest-connected live AI Chat agent (connection order, not tab selection). An explicit registration always takes priority. Connecting another AI Chat changes only future inbound delivery; existing requests keep their original reply peer. Request-specific reply tools require that peer’s MCP binding. Explicit registration takeover revokes pending requests. Only when neither exists does the bot send a short English registration instruction. An ordinary message after pairing in the same poll uses the new authorization; the pairing code never reaches the agent. Link confirmation is best-effort: if sending it fails, the chat stays linked and the consumed code is not replayed as agent mail. The failed reply appears in Recent messages.
3. **Recent messages** shows the last 20 events in the process serving Settings, newest first: time, in/out direction, message/pairing/start/reply/button kind, outcome, and a preview of at most 60 characters. It refreshes while the page is open. Codes, tokens and unauthorized text are redacted before storage. This log stays in memory, clears when the app or daemon restarts, and is not shared between a polling owner and a standby process. Check connection status, safe error category and last accepted message time. Status identifies **this app**, **tuic-remote**, or another desktop owner. The desktop app starts the same adapter at boot; its process-held lock prevents a second local poller. A standby takes over when the owner exits. If no poller is connected, Settings explains the next action and disables **Link chat**. Config changes restart the owning adapter and retire its current transient draft state. Resolve any reported error, then disable and enable Telegram to retry a stopped adapter.

## Shared provider account storage

Core now stores provider accounts and sites in one registry. Existing GitHub accounts retain their identities and repository bindings. Token values remain in the core credential store; account reads show only set, unset or unavailable status. The Accounts controls and private native add flow are integrated separately by the Settings extension. A failed account operation may need storage repair before retrying; see [migration and recovery](../backend/github.md#github-migration-and-recovery).

GitHub Enterprise account creation asks for the host in Settings, then opens a
separate private native window for the PAT. Settings never displays or stores
the PAT. Decline or close that window before submission to cancel. A headless
server cannot open this native credential window. After Submit is accepted,
verification finishes even if you close the window.

The internal GitHub settings backend keeps OAuth device login for github.com and
verifies Enterprise PAT identity before storage. Account reads expose credential
status only. The new shared settings tool is published in a separate cutover step;
this backend adapter does not add a new user-facing endpoint.

### Work-item access for agents

Agents can read linked Tasks items through the MCP repo tool. Tracker comments and status changes are disabled by default. To enable them, set `mcp_work_item_writes` to `true` through the configuration save API, or edit `config.json` while the app is stopped. There is no separate Tasks write toggle in Settings. A frontend with the provider loaded must remain connected. After a write times out, read the item before retrying.

### Background tray and menu-bar icon

In General > Confirmations, Tray and menu-bar icon selects Always, Only in background (default), or Never. Never disables the Background choice. If the tray cannot be created, or the Linux desktop has no tray host (such as stock GNOME), closing minimizes the window so it remains reachable. The tray menu offers Show TUICommander, live working/awaiting counts and Quit. An attention mark clears when you show the window. Linux uses the menu; macOS and Windows also support left-click to show.

## Plugin integration pages

Loaded plugins with registered settings appear under Settings > Integrations. Search includes their catalogue field labels. Unloading removes the page and search entries. Reads, revision-checked edits, resets and secret set/unset status use the scoped Rust settings service; the UI owns no second configuration store.

String-list edits, added rows, and removed rows stay in a draft until focus leaves the list. Read failures show an alert on the plugin page and do not close Settings.

### Automatic idle-agent hibernation

The backend checks unattended top-level agent tabs every five seconds. `agents.hibernate_idle_minutes` defaults to 60 awake minutes; zero disables the sweep. `agents.hibernate_scope` defaults to `background_only`; `always` also permits foreground hibernation. Both machine-owned controls are in Settings → Agents (Expert Mode).

Hibernation requires an idle agent and a verified resume session in its saved profile. Plain shells and agents without a verifiable resume session stay live. Unread peer mail, pending commands or background work, awaiting input, attached readers, keep-open tabs and open automation runs prevent hibernation. Managed children retain their separate idle-close policy. Sleep gaps longer than 30 seconds do not advance the idle clock.

The normal Rust suspension path persists `reason: "idle_hibernate"` before ending the PTY. Opening the tab, submitting input or sending peer mail uses the shared wake path. Storage or resume-verification failures leave the live session intact.

### Tasks notifications

Settings → Notifications → Tasks has **OS notifications for Tasks**, off by default and independent of the existing PR notification preference. Failed checks on a linked GitHub PR use the existing PR notice once and name the linked Tasks item. Click the notice to open the PR.

Use **Follow task** in an item detail to follow comments locally; use **Unfollow task** to stop. Follow state and the policy are saved in the existing Tasks UI preferences. A first observation establishes a silent baseline. Existing visible detail refreshes (including its 60-second refresh) and provider-bridge detail reads compare authoritative comments; there is no extra background poller. Your own comment action advances the baseline silently. Native notices recheck the Tasks policy after permission is granted and retain the existing background-window policy; browser clients do not send native OS notices.

GitHub provides a full normal-comment count, so an increase is detected even beyond the first page. Jira and GitLab provide paged comment entries: newly observed comments notify when their creation time is newer than the prior baseline, or when a previously unseen comment ID shares its latest timestamp. Unloaded pages, unseen review sources and items that are not refreshed cannot produce a notice. Editing a comment or loading older history does not notify. No Linear provider is shipped, so it is not covered.

## Settings MCP access (#1787-4778)

Enable the settings native tool under MCP to permit an agent to describe and edit settings. It is disabled by default, even if the removed config tool was enabled. Secrets are write-only. Changes to external access or the trusted executable require the existing confirmation dialog. The owner inventory is complete. Use explicit target/store/paths for owner reads and dynamic-choice describe/refresh. Typed workflows select their store and operation; credentials remain write-only. Future unresolved owner gaps block edits.


Agent model settings support typed model edits without starting a CLI. Explicit
choice refresh probes only the addressed Claude run config with `--help`, using
its command and environment. Published aliases are suggestions, not account
entitlement or an exhaustive model list. Other CLIs and failed probes report
unavailable choices; typed edits remain supported. Describe never probes. Model
edits preserve sibling configs and the default selection and apply next session
(#1910-451e).

Shortcut saves reject invalid action identities, duplicate action records and invalid modifier syntax. Plugin shortcuts retain stable namespaced IDs. Pane geometry saves reject invalid ratios, excessive nesting and missing groups without changing saved layout. Runtime tab membership and current pane focus remain session restoration state.

Font options come from the bundled font catalogue. Theme options include custom themes from the Rust theme owner. Settings choice discovery is explicit: describe does not probe installed applications; refresh requests host IDE discovery. Unavailable discovery is not an empty result.

### Audio device availability

An empty device list means enumeration succeeded with no devices. If the audio backend cannot enumerate devices, refresh reports an error instead. Voice language and engine changes keep the existing speech runtime invalidation behavior.

### Shared layout

Settings uses one bounded, responsive shell across pages, with independently scrolling navigation and content. The panel is at most 1136px wide and retains its size when you select another page. Saved navigation width remains in use. Shared presentation components support consistent cards, label/control rows, source-aware status, empty states and scoped destructive actions as pages adopt the approved restyle. Save behavior and configuration ownership do not change.

### Scope and operation states

The page header identifies the setting owner. Repository edits remain tied to
the selected repository; Agents uses its machine selector; plugin pages name
the plugin that supplies their fields. A page change does not transfer settings
between these owners. **Done** closes Settings; it is not a global Apply action.

Loading, empty results and failed reads are different states. A plugin catalogue
with no fields adds no page. While plugin values load, the editor shows a local
loading message and keeps navigation and Done available. A failed read shows an
alert inside the editor. Rejected writes reload the latest revision rather than
reuse stale values for the next edit. Unloading a plugin removes its editor even
if a read is still pending.

Status words describe the reported state, not inferred connectivity. Saved
credentials do not prove live access, and ego's unchecked sandbox status is not
proof of OS enforcement. Use the page's existing Refresh or recovery action;
opening Settings does not authorize extra network or sandbox probes.

Section links open the requested page and scroll after it mounts. Older
`services` links open MCP, and `developer-tools` links open General. A missing
section does not redirect to an unrelated control. Closing Settings cancels a
pending section scroll.

### Task-based settings presentation

AI Chat groups new-run defaults into **Agent and model**, **Workspace access**,
**Network and sandbox**, and closed **Advanced and diagnostics**. The executable,
profile, and workspace remain owned by TUICommander in General. Model and access
values remain owned by ego; these controls do not change running conversations.

Voice uses separate task cards for dictation, recognition, corrections,
hands-free conversations, and spoken replies. Smart Prompts retains categorized
prompt editors and the existing execution controls. Integration pages use local
connection, access, and service cards. Remote token regeneration requires a scoped
confirmation because it disconnects active remote sessions.

The plugin manager retains lazy Installed/Browse views. A loaded plugin's settings
page identifies its plugin source and uses schema-driven controls; saved credentials
are not evidence of a live connection. Existing explicit-save and automatic-save
boundaries still apply to each page.

Loaded plugins with described settings appear in a separate **Plugin settings**
navigation group with a plug icon. The manager offers **Settings** only when the
loaded catalogue provides a page. That action opens the same scoped page as the
navigation entry; it does not create a second settings service.

AI Chat groups provider details in **Other providers and available models**. Workspace access shows the effective folders first. **Add folder** and **Edit** reveal the existing ego roots draft; **Save roots in ego** keeps its explicit save boundary. The network selector writes through ego. **How to verify** explains command-isolation evidence without running a probe.

Voice keeps downloadable models and voice management in **Speech assets**. Remote Access separates connection status, Tailscale HTTPS and token revocation. Plugin rows keep secondary controls in **Manage plugin**; plugin pages identify their source and plugin-owned scope.
Native MCP tool help stays anchored during status refreshes. Focus a `?` badge
with the keyboard or hover over it to read the full tool description. Help
placement follows panel scrolling, window resizing, and description size changes.

### Contextual explanations

Selected settings show a `?` only when the approved explanation adds decision guidance, requirements, consequences or owner-dependent defaults. Hover or focus the badge to read it. Click or press Enter/Space to pin it; click again, press Escape or click outside to dismiss it. Help does not save a setting, contact a service or run a sandbox probe. Long help scrolls inside the viewport and follows panel scroll/resize. Native MCP help retains tool-name identity while descriptions update during status polls (1928-e282).

AI Chat places provider refresh beside the default model and lists effective roots with their source, access and Edit action. Reload from ego is in Advanced and diagnostics. Voice lists Whisper downloads below the active-model summary and keeps correction import/export in the card actions. Smart Prompts uses Edit to open a separate editor card; the enable switch stays visible on each library row. Custom prompt deletion remains confirmed in the final danger section.

Loaded plugin pages use the same title-icon slot as other Settings pages. The
source line identifies bundled or installed ownership without changing the
page label, catalogue lifecycle, or scoped editor behavior.

Plugin field resets sit next to their controls and appear only for non-default
values (or a set credential). Plugin management uses compact secondary actions.

### Telegram request streaming (1959-162a)

Telegram replies now use TUIC-driven receipt (👀), working (👨‍💻), waiting (🤔), answered (👍) and failed (👎) reactions. An ephemeral native draft shows Italian status lines or unapproved agent previews; it refreshes while work continues and the final reply replaces it. Acknowledgement of work requires inbox Read and a new recipient turn. PTY intent/activity and AI Chat assistant chunks share request correlation; finish remains authoritative after draft retirement. Telegram-mail-woken AI Chat turns automatically send their settled assistant text if finish has not answered. Queue and no-reply notices occur at ten minutes and sixty seconds after Read respectively. Restart keeps no request history. Phone rendering requires a live check after the backend restart.

Settings search includes page titles and shared card controls. Page-title results scroll to the page header; when a page and card share a title, control results use the card. Runtime status badges and summaries are not settings search targets.

### Changes made through MCP

An open Settings page refreshes after a durable MCP settings write. Unsaved fields stay in place; if another writer changes the same field, a warning asks you to review your edit before saving again. Incoming changes do not run startup migrations or save the snapshot back to the backend. A reconnected browser rereads its active settings sections, including after a server restart. Remote-machine changes refresh only that machine's editors, not local settings. If a write persisted but its runtime apply failed, Settings still rereads the saved state; the caller must follow the returned recovery action rather than blindly replay the write.

## All settings (Expert)

The **Application → All settings (Expert)** page provides typed scalar and string-list editors from the Rust catalogue. Search results update the inspector even when this page is already open. You can also select a setting manually. Bind any stable record keys, and select a registered repository or existing Ego profile when required. The inspector edits existing records; use the dedicated domain page to create records, manage credentials or edit structured pane/forward/root data. It does not expose an opaque JSON editor.

Reads show owner state and a revision. Save changes only that field with its read revision. A remote update keeps a dirty draft and its old revision, so a conflicting save cannot silently overwrite the new state. **Discard draft and reload** explicitly abandons that draft. Exposure/execution edits first return a proposal; **Ask for confirmation** uses the existing human dialog. If persistence succeeded but runtime application failed, or the transport lost the outcome, the inspector disables writes until you reload authoritative state. Read back before retrying.

General also exposes content-index memory budget, attachment byte limit and background WebView unload delay as expert resource controls. Enum-backed shared selects use catalogue values while retaining their existing readable labels. Search includes the inspector descriptors. In browser mode, these results open machine-owned fields in the HTTP-capable inspector; they do not open desktop-only controls such as the native global-hotkey recorder.

The explicit source ledgers distinguish read-only identities, derived credentials, legacy snapshots and separate domain controls. CircleCI credentials use the write-only `circleci.token` owner. Dictation corrections use stable heard-text keys in the machine-owned `corrections` collection; edits reuse TextCorrector and refresh open Voice pages without microphone access. Global PR visibility filters persist in config.json. Nullable repository overrides persist in repo-settings.json; null inherits the global filter and false explicitly shows matching PRs.

## Assisted startup

The [Settings startup contract](../backend/settings-startup.md) describes discoverable GitHub device/Enterprise workflows and Ego settings/readback. GitHub `list_accounts` discovers existing account IDs and credential presence. Typed Ego `begin_login` opens a terminal for you to authenticate; `login_status` reports process lifetime and owner credential readback separately. Native inventory completeness does not mean every onboarding or live UI check is complete.

### CircleCI Settings owner (#1991-bd88)

Settings discovery includes the write-only machine setting `circleci.token`.
Typed `set_token` and `clear_token` actions share the existing token owner across
MCP, IPC and HTTP. Readback reports stored presence only (`set`, `unset`,
`unavailable`), never a secret. Git & GitHub keeps its effective-source status;
removing a stored token leaves environment and CircleCI CLI fallbacks unchanged.

On narrow screens, search and page selection share one compact navigation row. The workspace path in AI Chat is a read-only field; select or scroll its text to inspect the full path.

### Mobile font sizes

In mobile Settings, use the Terminal and Chat minus/plus controls under Appearance to set each size from 10 to 24 px. Each value is saved on this device. Terminal changes CLI output; Chat changes Session Chat and AI Chat transcripts. The defaults remain 14 px for terminal output and Chat body text. Composers, UI controls and desktop fonts stay unchanged. CLI text wraps to the available width without changing the shared terminal size.

### Abandoned shells

Desktop sidebar session rows and mobile session cards mark a parentless plain shell as **Abandoned** after one idle hour and show its idle age. The five-second backend sweep requires a fresh OS process inventory: the PTY child must be an interactive shell with no child processes. An unknown inventory, script, foreground command, background child, detached TUIC job, managed identity, live child session, pending input or keep-open hold prevents closure. Unsupported or incomplete OS process observations fail closed.

Settings → Agents → Idle agent hibernation (Expert Mode) includes **Close abandoned shells after (hours)** (`agents.abandoned_shell_hours`). The machine-owned value defaults to **0 (off)**; whole hours from 1 to 65535 enable closure. Idle age is measured from the most recent input, output or semantic activity, including elapsed time while the machine sleeps. Closure rechecks the process inventory and activity at the destructive boundary, and uses the normal session close/tombstone path. The sweep can lag the threshold by one five-second tick. A remote daemon applies its own setting, not the desktop host's value.

The monitored-repository count tracks successful snapshots in the running GitHub monitor, including repositories with no open PRs. Query-cache expiry and temporary poll failures do not reset this count. Removing a repository or stopping monitoring removes its entry.

Git & GitHub access diagnostics show exact per-repository messages grouped by owner and account. For github.com OAuth accounts, use the OAuth app authorization action, then select Refresh status to clear NOT_FOUND retry delays for all accounts before the next monitoring poll. Opening Settings does not clear these delays.
