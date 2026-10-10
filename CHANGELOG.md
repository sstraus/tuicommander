# Changelog

All notable changes to TUICommander will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

- Widen the Activity Dashboard popup by 20%, while keeping it within small windows (#1664-f4ab).

- Add Spoken replies to mobile hands-free and Settings; muting keeps dictation active, and Edge HTTP 401/403 stops repeat requests for five minutes with one text outage notice (#1659-f3cc).
- Give mobile toolbar keys enabled primary text and highlight Ctrl while its menu is open (#1663-8c02).

- Use the terminal composer pin and play icon buttons in desktop AI Chat, with named Queue and parked-draft states (#1636-0d08).

- Accept pasted images in AI Chat before ego connects, retain concurrent pastes while its first session opens, and share Finder/text clipboard rules with Ideas and terminal Compose (#1639-f474).
- Add the `automations` MCP tool for definition management, with bound creator provenance and atomic pause/resume.
### Added
- Start the owner-guarded Once automation runtime on desktop and headless hosts, with durable precheck decisions, literal prompts and configured workspace/profile dispatch. Public execution controls and completion handling remain pending (#1615-57cf, #1614-7e8f).

- Mobile terminal Ctrl menu offers Ctrl+C, Ctrl+B, Ctrl+D and Ctrl+Enter from one keybar button. Ctrl+Enter follows the agent registry mapping; verified behavior and conservative fallbacks are recorded with the probe evidence.
- Add the Rust foundation for Once automation schedules with IANA timezone validation, a single preview instant and durable occurrence deduplication (#1629-2262).

### Fixed

- Restore external URL tabs by removing the restrictive `frame-src` CSP so frames inherit the permissive `default-src`.

- Keep hands-free PTY echoes out of the mobile composer, including delayed echoes after typing or reconnecting; preserve explicit Tab completion and editable history recall (#1662-c549).
- Keep mobile Claude tool-call text and wrapped continuations stationary while its status dot pulses or turns green (#1654-a3d5).
- Preserve line breaks between held hands-free phrases and submit the combined turn once (#1656-eb5e).
- Honor Open Desktop UI on iPad, remember the device's choice, and keep a visible return link to mobile in the desktop shell (#1658-fb34).
- Keep terminal Chat following when Claude replaces a transcript at the same path, when it has no subagents, or after a rapid CLI/Chat remount; log failed refreshes or stopped transcript tickers (#1635-3db2).
- Repair stale TUIC MCP bridge paths in Claude private and configured launch profiles, and warn when a missing configured bridge cannot be repaired (#1637-d6e1).
- Search the visible Claude Chat transcript with Cmd/Ctrl+F; matches span inline Markdown and skip collapsed thinking; Enter and Shift+Enter navigate matches, and switching CLI/Chat closes search and clears its highlight (#1633-f2cc).
- Show Claude prompts typed mid-turn in terminal Chat, while keeping task notifications and queue metadata hidden (#1632-ec84).
- Keep mobile AI Chat within the phone width and use the session composer styles with labelled icon actions (#1625-c04f).
- Give the mobile update banner AA text contrast in both themes and place it below the header, away from the status edge (#1626-9529).
- Fix mobile session notification sounds playing outside the visible Sessions list, including delayed completion sounds and hidden PWA documents. Keep muted transitions from replaying on return.
- Mobile slash keybar now inserts `/` like typing and uses the agent’s detected command menu, preserving surrounding draft text.
- Restore headless builds by sharing GitHub account login validation with browser device-flow polling.
- Keep browser GitHub add-account polling separate from default login; emit declared plugin state changes, deliver SDK host messages, close panels on unload, and reject HTTP plugin manifests without allowed URLs.
- Remove clean worktrees whose ignored directories lack owner search permission, without changing permissions on symlink targets (#1607-0733).

- Preserve the next unsent Compose draft when a send or queue completes after leaving terminal Chat, including when the editor has already reopened (#1599-db8a).

- Launch the Design Mode browser without chromey's `--enable-automation` and `--disable-extensions`, so it shows no automation infobar and can load extensions (#1593-987f).

- Keep sidebar wheel and clicks working while the status-bar ticker popover and info balloon, the GitHub panel and the Smart Prompts dropdown are open; their invisible full-window backdrops are replaced by one shared outside-press dismissal that does not consume the press (#1597-fab6).
- Keep sidebar wheel scrolling available while the PR detail popover is open; outside presses dismiss the popover without blocking the underlying control (#1596-df57).
- Remove terminal-grid prompt tint because recorded cursor rows can identify the composer or tool output; keep prompt colours in Chat (#1595-22b1).

- Reject HTML responses to remote JSON RPCs with the endpoint and content type, so Add Repository shows the error instead of an HTML home path (#1594-f297).

- Remote desktop terminal tabs now receive daemon agent identity, parsed state and OSC title/reset notifications through the owning connection (#1581-8fd0).


### Added
- Add the Automations dialog with backend cadence and Once previews, recent history, atomic pause/resume, Run now and confirmed deletion. Backend API integration is pending (#1618-685b).
- Add an all-repositories toggle to the Ideas panel header: it lists every repository's ideas for cross-repo work (in memory, resets on reload). The badge and "clear completed" follow the visible list, so clearing in a single-repository view no longer deletes other repositories' completed ideas; ideas added in all-mode stay tagged with the active repository (#1598-c0a0).
- Add a headless agent capture scenario driver with explicit fixture promotion and ordered replay expectations; record real Codex and pi turns; Claude (expired login) and goose (its banner prints the home directory) stay unverified, and a test rejects fixtures that embed `/Users/` or `/home/` paths.

- Telegram draft Stop interrupts only its bound agent turn; replacement input retires stale Stop requests before writing.
- **Mobile Markdown review** — In the mobile Files view, tapping a task-list checkbox toggles it in the file, and a block comment can be added to a paragraph, heading or list item in the desktop's `<!--tweak:...-->` format. Saves refuse to overwrite a file that changed on disk meanwhile (new `write_file_if_unchanged` command, `POST /fs/write-if-unchanged`).
- Add a CLI | Chat switch to Claude terminals: a conversation view built from the agent's session file, with prompts apart from replies and tool calls folded into compact cards. The grid is hidden while Chat is open and never unmounted.
- Fix missing older Claude prompts, image/PDF tool-output markers and model-change cards in Chat; validate the parser against sanitized records from real CLI transcripts.

### Changed
- Upgrade Mermaid to 12.1.0: diagrams use its new layout and appearance defaults while retaining the renderer's dark theme and strict security setting (#1590-7c6b).
- Require macOS 12 Monterey or later for the desktop bundle, with Safari 17.4 or later for Mermaid diagrams (#1590-7c6b).
- **Remote upload credentials** — `POST /remote/update` and `/fs/upload-copy` accept only the `tui-session` cookie; a `?token=` query answers 401 and the update client no longer puts the token in the URL. Older desktops cannot update a daemon of this release. QR pairing and WebSocket authentication are unchanged.
- Make Compose the default input in terminal Chat: it opens focused and docked, stays open after send or queue, and preserves each tab's CLI Compose state and unsent draft when switching views (#1599-db8a).
- Space automatic sidebar refresh starts at least five seconds apart per repository, retaining one trailing update during event bursts; explicit repository additions, checkouts and merge cleanup bypass the wait.
- Resolve browser HTML and Markdown images through open repositories; show a clear placeholder for external images.
- Show the existing ACP refusal text once in a plain-text AI Chat card.
- Integrate workflow approval, plan dispatch and public story controls; public plan API starts share the daemon executor. Canonical workflow test fixtures preserve Windows ownership.

- Dispatch bounded plan story waves, wait for explicit dependency integration, and require current deterministic final checks before completion.

- Run pre-approval workflow checks and record independent reviewer approval; retain implementer role restrictions after exit. Execute deterministic story Gate outcomes.

- Execute pinned serial workflow Agent/Judge/Loop/Pause/Notify/Join nodes in the owning daemon with durable reports and effects; refuse unsupported node publication. Manual pause time no longer consumes active run duration. Public graph start controls remain in development.
- Start published story workflows from Plans and Stories; inspect pinned graph positions, decision evidence and full paged events. Add explicit graph resume, pause/cancel controls and the generated `workflow_run` MCP surface. Plan start remains visibly unavailable pending dispatch.

- Execute pinned serial workflow Agent/Judge/Loop/Pause/Notify/Join nodes in the owning daemon with durable reports and effects; refuse unsupported node publication. Manual pause time no longer consumes active run duration. Full plan dispatch remains in development.
- **AI Chat steering** — Text sent during a running turn joins the current query when ego advertises ACP steering, on desktop and mobile. Attachments and unsupported or rejected steering retain the queue. Delayed steering responses preserve submission order with later text and attachments.
- Add recorded incident causes and manual next-step suggestions to Plans and Stories run history.
- Add persisted ego profile, workspace and executable options for individual AI Chat conversations, including HTTP creation and reopen.
- Audit MCP inbox reads with caller, owner and message ids without logging mail bodies.
- Let MCP agents page retained terminal output without rerunning commands. Responses explain the next page, raw byte offsets work locally and remotely, and tiny raw pages cannot reconstruct masked secrets.
- Bound repository-selected ego profiles by the machine profile over ACP and show ego's clamp warnings in AI Chat.
- Add caller-bound MCP worktree declarations for agents using `git -C`, with same-repository validation and placement restored after restart.

- Place only the same-repository MCP caller in its new worktree when no separate session is requested; preserve sibling tabs and inactive selection.
- Separate active and idle ungrouped repositories in the rich sidebar and show working-agent counts.
- Keep rich sidebar navigation dense at iPad width and fold returned subagent history.
- Extend intent only for working rich sidebar agents; keep idle rows on one line and full intent in tooltips.
- Fold old idle sessions in the rich sidebar without changing session order.
- Hide zero PR and worktree counts in rich sidebar repository metadata.

- **Workflow daemon foundation** — Recover runs only after acquiring database ownership, retain durable root positions and story reservations, require explicit graph resume, and pause idle runs at their duration deadline. Executable Agent delivery remains disabled pending later slices.

- **Workflow graph foundation** — Durable serial graph positions, bounded predecessor history, decision provenance and repair counters now replay from pinned definitions. Executable validation requires final checks and explicit pause targets. This slice does not enable autonomous scheduling; older pre-contract runs remain inspectable and cancellable.
- Fix command boundaries, prompt ticks and answers-only prompt association drifting after scrollback eviction.
### Added

- **ego launch permissions** — Choose permission mode and filesystem sandbox in Settings > Agents > ego. Terminal launches and MCP spawns use the same persisted overrides; leaving either choice unset preserves ego configuration.
- Inspect managed-agent launch instructions from the terminal context menu: captured brief (including peer context), explicit system instruction arguments/file snapshots and MCP initialize responses, with source, original UTF-8 bytes, redaction and capture limits. Historical and agent-autoloaded instructions are explicitly unobservable.
- Add ego Perimeter controls in Settings > AI Chat for root access, read allowlists, extra writable directories and network mode. Show ego's effective view and a measured or unverified exec-sandbox badge; writes use ego CLI.

- Add terminal history back/forward (`Cmd+Alt+Left/Right`, toolbar arrows) over the last 10 activated terminals, and a Default/Recent sort on the mobile Sessions screen.
- Navigate from parent agents to live children in other repositories.

- Summarize merged idle worktrees with a guarded Clean up action.

- Keep rich active/idle repository sections inside their existing groups.

- Add an opt-in Telegram channel to the headless daemon with native peer mail, correlated Thinking/activity drafts, exact final replies, authored progress notices and opaque-button choices. Agents opt in with `telegram register`; registration ends with the agent or MCP session and is not persisted. With no agent, authorized chats receive "Nessun agent registrato". Live mint deployment remains pending.

### Fixed
- Prevent primary terminal presses from temporarily blurring the shared keyboard input through native canvas focus. Physical iPad keyboard retention remains to be verified.

- Keep backend memory census on demand, use cheap watchdog summaries, and identify census bytes as overlapping map accounting.
- Preserve live AI Chat response chunks when reopening a configured conversation, including refused reopen requests.
- Clear session-specific terminal stream notices after replay recovery; show reconnect progress through the existing ten-attempt budget.
- Keep Settings native MCP tool rows concise with dedicated English summaries; preserve full tool descriptions in their information tooltips.
- Release the worktree removal lock when its confirmation dialog fails, so a later removal can proceed.
- **Tablet terminal input** — Touch taps use the main keyboard input instead of an offscreen textarea, avoiding a focus switch on subsequent mouse events and sharing soft-keyboard deletion handling.
- Reap PTY children that exit after terminal EOF or session removal, preventing zombie shells in the headless daemon and desktop backend.
- Return freed terminal scrollback pages to the OS after session cleanup on Linux/glibc instead of retaining them in allocator arenas.
- Keep managed Claude background-task output under `~/Gits/.tmp/claude` by default, preserving an explicit `CLAUDE_CODE_TMPDIR`.

- Start sidebar agents when shell readiness arrives before launch preparation or the remote terminal subscription.

- Return from captured Windows background launches before the command exits, and resolve Git in Windows worktree hooks.

- Match Claude transcript and subagent directory names for Windows profiles and checkout paths containing spaces or punctuation.

- Match Windows worktree paths across Git and native APIs, support deep Git paths, and close upload staging handles before cleanup.
- Stop workflow check process trees on cancellation and shutdown; record story transition provenance without restricting trusted local workflow actions.
- Run Git freshness probes outside workflow and story write transactions, rejecting concurrent persisted revision changes.
- Keep workflow plans Active until approved stories have current integration receipts, and reopen them after canonical ref movement.
- Background completion queue retries reuse a stable key, preventing duplicate wakes after a lost reply; exhausted ambiguous replies remain uncertain even when mail fallback fails.
- Retire Telegram opt-in on observed agent exit before a replacement in the same terminal can inherit it.
- Keep CLI and MCP secret requests connected while users enter values or approve commands; add safe native form creation diagnostics.
- Launch macOS `make dev` from a stable executable copy outside Cargo/mbx targets so target cleanup cannot orphan the running app's firewall identity.
- Show every registered native MCP tool in Settings, including disabled tools, using backend descriptions and switches that follow the existing MCP access filters.

- Bound nightly release notes and link to the full comparison when the commit list exceeds the release body budget.
- Expose the Codex approvals/sandbox bypass in its default run configuration with a warning icon; removing the argument stays effective across restart, older-backend saves, and managed launches. Codex wrapper defaults retain positional task delivery; interactive profiles named `review`, `exec`, or `e` retain submitted task delivery.
- Initialise headless VT screens at the requested terminal width.

- Preserve Claude subcommand arguments without injecting launch-scoped settings.
- Deliver remote MCP confirmation dialogs and their resolution to the desktop. Keep newer remote ACP questions visible when an earlier answer completes.
- Preserve AI Chat replies with parentheses and remote permission snapshots when unrelated card notices arrive.
- Restore additions and deletions in the changed-file diff list, including renamed files.

- Return the exact archive ref holding a deleted branch tip when a reused branch name needs a suffixed archive.

- Publish the Codex notify script with execute permission already set, including concurrent desktop and daemon starts.
- Open terminal directory links in the File Browser and report missing paths without creating empty editor tabs.

- Build bundled WebRTC with Visual Studio for Windows MSVC targets even when MinGW is on `PATH`.
- Restore the Windows process environment API dependency in `tuic-core` after the workspace split.

## [1.8.0] - 2026-10-04

- **Private secret forms** — Agents request only named sensitive fields and run user-approved argv commands with values in the child environment. Values remain in memory, output is masked, and a one-time capability supports browser or phone entry on the existing server. TUIC inspection tools are suspended while entry is open.
- **Remote peer mail** — A flooding host has its own replay quota. Budget pressure reclaims only departed senders, and disconnect retires old shadow peers even during a reconnect handshake. A departed sender may replay once after pressure evicted its history.

### Fixed

- Desktop MCP now discovers and addresses configured remote terminals and peers, preserving native output and semantic submit behavior. Authenticated peer mail supports remote replies and remote-to-remote delivery through the desktop hub.
- **Workflow recovery** — Recover each database once per process and continue past individual run failures. Later opens and runtime reconciliation preserve resumed workers instead of retrying restart recovery.

- **Remote file drops** — Dropping Mac files onto a connected remote repository copies them through the authenticated daemon connection. Directory drops keep the recursion confirmation; existing names are skipped. Uploads have a 17-minute total receive deadline, and read-only directories publish correctly. Abrupt daemon termination can leave a staging directory for manual removal.
- **HTTP API security** — Reject foreign browser origins and DNS-rebinding Host names before handlers run. Protected HTTP requests now require credentials even from loopback/LAN; local CLI/MCP IPC retains its existing access.

- **Remote agent notices** — MCP toasts from connected daemons reach the desktop notification bell with the host name, original level and requested sound. Open terminal selects the originating remote tab; unknown sessions leave focus unchanged. Disconnected notices are not queued for replay. Malformed remote notice text is discarded, and identically named connections retain separate notices.
- **Telegram adapter groundwork** — Add offline owner, file authorization, safe Bot API and minimal inbound boundaries. Persist only the atomic polling cursor; unread inbox mail may be lost on restart. Harden offline polling with fixed ten-update batches under a 1 MiB response cap, one cursor write after each successful batch, shared retry scheduling and in-memory rejection stops. Daemon polling and native mail delivery remain disabled pending integration; no live Telegram channel is available yet.

- **MCP Markdown tabs** — Documents opened with `ui action=tab` survive UI reloads in the same window, including native recovery without an unload event. Reopening their stable id updates the restored tab without creating a duplicate.
- **Remote terminal stream failures** — A stalled initial replay, unreadable frame or disconnected stream now shows a persistent error toast. Reconnect attempts remain bounded until a frame reaches the renderer.
- **Answers-only view** — Keep marked answers that precede the first tracked prompt. Show a short notice when no marked answers remain in the retained history.
- **Stable MCP bridge** — Agent configs use an atomically installed bridge outside Cargo targets. Rebuilds and target cleanup no longer remove the executable needed by new MCP connections; previous bridge revisions remain available through updates.
- **Remote agent detection** — Agents started by hand in a headless terminal gain backend agent state, safe submit and mail wake. HTTP and desktop foreground detection now share the state update. Rejected submissions and unavailable mail wakes explain the cause and the next action.
- Revoke agent identity when the foreground returns to a shell after the agent was observed, retaining startup presets and transient helpers; recognise native versioned and updater-replaced Claude executables on Linux.

- **Hands-free controls** — Start conversation uses the accent action style. A coloured Running/Stopped indicator keeps the current state visible.

- **Sidebar agents toggle** — A separate, always-visible chevron controls the agent list. The status icon selects the branch, and collapsed counts stay beside the chevron.
- **Bodyless IPC replies** — CLI and bridge response decoding completes at the headers for HTTP 1xx, 204 and 304, and CLI HEAD replies, instead of waiting for a body or connection EOF.

- **Named-instance IPC clients** — `tuic` and `tuic-bridge` now select the server's instance socket with `--instance <id>` or `TUIC_APP_INSTANCE`. The CLI background runner keeps the same namespace, and bridge fallback discovery cannot connect to another instance.
- **Claude prompt copy** — Copying a wrapped prompt removes the composer marker and continuation margin, joins width-supported wraps, and retains typed newlines and pasted content. Cleanup requires a column-zero composer origin; partial selections and VT soft-wrap continuations retain literal prompt glyphs and indentation An oldest retained row with an unknown predecessor stays literal until fully erased and rewritten (#1411-097c, #1414-4366, #1418-48c7).

- **Suspend Tab** — Right-click a terminal tab → Suspend Tab ends its process and agent to free memory and CPU, and keeps the tab (marked `zz`) restorable like after a restart. Resume Tab, or the Resume button in the tab, opens a new session in the same folder and resumes the agent. A suspended tab stays suspended across a restart. Suspend is refused while the agent is working or a question awaits input. MCP: `session action=suspend` returns the tab's verdict, or an error when no UI is attached. Unlike auto-standby, which only pauses an idle process, a suspended tab holds no process.

- **Branch integration proofs** — MCP can list integration verdicts for all local branches or query one branch, including its worktrees and proof. The sidebar and branch panel recognise squash merges, no-op merges and revised twin patches. Content-based verdicts require an archive of the current tip before safe deletion.

- **AI Chat toasts and bubbles** — Toasts keep clear of the docked AI Chat panel and return to their usual corner when it closes or is detached. A short user message no longer shows an empty second line: its Copy button moved outside the bubble.
- **ego Login from Settings** — Each provider under Settings → AI Chat has a Login button that opens a terminal tab running `ego auth login <provider>`; the key never passes through TUICommander.

- **Safe orphan cleanup countdown** — Clean detached worktrees are removed after a configurable countdown in the Ask dialog. Dirty or untracked worktrees and commits not reachable from a branch require explicit review. Agents can answer the pending dialog through MCP, with a fresh backend safety check before removal.

- **Managed child idle close** — Idle managed agent children close after a configurable delay, while unread mail, background work, keep-open, or a failed background wake keeps them available.
- **Orphan worktree cleanup** — The status message counts only successful removals and does not claim success when cleanup fails.

- **Native desktop alerts** — Agent questions and Progress outcomes send system notifications while TUICommander is unfocused, with a short duplicate suppression window. Clicking one on macOS returns to its terminal or Progress project. The macOS sender accepts repeated requests without holding a backend thread per unanswered alert.
- **Background command wakes** — `tuic bg` retries when a transient queue error and failed mail leave the caller unwoken, records the originating session and attempt count in `.wake`, and preserves the final error after its retry limit.
- **Queued Codex confirmation** — A stop hook can delay the Working screen for several seconds. Queued submissions now wait long enough for that confirmation before showing an uncertain-delivery error.
- **Queued Claude confirmation** — Claude can hold a submitted notice in its own queue for several seconds. TUIC now waits for its busy hook or Working screen before showing an uncertain-delivery error.

- **Claude questions on mobile** — The session shows every AskUserQuestion option and offers those choices instead of generic Yes/No. Tapping a choice navigates Claude's menu and submits it with Enter.

- **Mobile Codex questions** — Open queued questions from the session header, see their choices, and answer with one tap or typed notes. The question overlay does not reduce terminal space.

- **CLI MCP worktree timeouts** — On Unix, `tuic mcp` and `tuic repo` wait up to 305 seconds for worktree creation and removal. Other commands retain their short timeout. A timed-out MCP request now warns that the server may still complete it, so callers can inspect state before retrying.

- **Mobile Markdown images** — Relative images in repository Markdown now load from the file's directory through an authenticated image route. Paths outside the selected repository and non-image files are refused.
- **Mobile Progress, Activity and Settings** — Long journal messages expand on demand, Activity shows local times and minute durations, and Settings shows app and server versions and offers a light theme saved separately from the desktop theme in server preferences.

- **Mobile Files** — Repository paths keep their end visible and reveal the full path on long press. The tree puts hidden folders after normal ones and searches nested files. File actions share the title row; the editor fills the available height and wraps lines.
- **Compact mobile session header** — A 56 px row shows the agent logo, state, session name, and tasks count. Intent, tasks, and session progress open temporary sheets; Files, output search, Ideas, and quick commands remain reachable from overflow without reducing terminal space.

- **AI Chat prompt parking** — `Ctrl+S` parks a draft and its images while you ask something else; the parked draft returns after Send. The composer control also works on phones.

- **Mobile slash commands** — Typed and menu-selected commands wait for the agent's input gap before Enter, so Codex accepts the submission. The Codex quick-command widget offers `/status` in place of unsupported `/help`.

- **Mobile session links** — Markdown references in terminal output open the mobile Files editor, while web links open in the phone's browser. A path outside registered repositories shows a refusal with the path.

- **Mobile session search** — A magnifier above the session list filters by session name, repository, worktree, branch, or agent.

- **Concurrent settings saves** — Editing different settings in overlapping windows now preserves both changes across app, agent, notification, UI, repository, remote connection, prompt, notes, activity, keybinding, dictation, and GitHub account and binding configuration.

- **MCP current tool-call results** — The stdio bridge now returns `resultType: "complete"` for 2026-07-28 tool calls, including tool-level errors, so current-protocol clients can use TUIC tools. Legacy tool-call responses keep their existing shape.

- **Worktree removal with read-only build artifacts** — Removal now makes files inside the checkout owner-writable before Git deletes them, and names a leftover directory when cleanup cannot complete.
- **Manual story dependencies** — Approving a prerequisite immediately releases eligible dependents in plans without a workflow run. Workflow-owned plans continue to require current integration receipts. Active-run reconciliation now runs on first workflow use after desktop or headless startup.

- **Native story review** — Independent managed reviewers and local API clients can approve stories with recorded actor provenance; the session that claimed a story cannot approve its own work. The browser dialog offers the same approval action as desktop.

- **Workflow worktree assignments** — Runs accept registered branch worktrees for story attempts while rejecting paths outside the repository's registered checkouts.

- **ACP peer mail receipts** — Sending normal or urgent mail to an ego peer with a subscribed MCP-over-ACP inbox now reports the ACP delivery route, including when it is registered as an orchestrator; disconnected peers still report inbox-only.

- **Worktree removal review** — Confirmations identify untouched branches, live sessions, and local file counts. One-click Merge & Archive asks before moving a live agent's checkout. MCP and HTTP removal report the same hazards; automatic PR-close cleanup and merged-worktree archiving skip live or dirty worktrees and report why.

- **Automatic remote daemon updates** — An optional per-machine setting updates an idle outdated daemon after connection. The update survives the daemon's restart, retries when a changing session count settles, and refreshes live session counts while connected. Stalled transfers time out. The manual update action is disabled during an automatic update. Manual and automatic updates claim the same connection, so concurrent requests are rejected and both appear as in progress.

- **Worktree cleanup after external removal** — Removing a checkout already unregistered by Git now cancels its pending build-input copy even when leftover directory cleanup fails.

- **Push-to-talk sustained-speech gate** — The final pass uses the hands-free frame and duration rule to skip short noise bursts before Whisper. Its activity floor follows the configured transcription RMS floor so quiet sustained speech can still reach Whisper.

- **Push-to-talk skip reasons** — When the final transcription rejects audio, the dictation status now shows the specific speech-gate reason instead of always saying “no speech detected”.

- **One AI Chat across repositories** — The AI Chat is the same conversation whichever repository is on screen: switching repository keeps its tabs and starts nothing, and the repository you are viewing is sent with each message as context. ego starts with your first message (or **+**), runs in `~/Gits`, is shared by every window and the phone, and is ended when TUICommander quits.

- **ego reaches TUICommander over ACP** — The AI Chat's ego sessions now get the `tuicommander` MCP server on the ACP connection itself (MCP-over-ACP) instead of a `tuic-bridge` process. Each tool call no longer opens a fresh HTTP MCP session; the tools, their names and ego's permission rules are unchanged, and ego can cancel a running call. Claude Code, Codex and other agents keep using the bridge and HTTP `/mcp`.

- **AI Chat replay storm** — A chat tab replays its conversation at most once at a time, and a failed replay waits for the person to select the tab or press Retry instead of repeating on every update. The backend refuses a second `session/load` or `session/resume` while one is pending, and logs every attach with its method and session id.

- **AI Chat layout and composer** — Collapsed tool activity keeps commands in the expanded view, message Copy has reserved space, and streaming output follows the bottom until you scroll up. The composer grows with text and condenses pastes over 200 words into a marker until sending.

- **New worktree tabs** — A session spawned before its worktree appears in the repository list moves under the registered repository after a refresh, without a misleading Register toast.

- **SSH remote readiness** — Tunnels become Connected after SSH survives startup and every local forward accepts connections. Installed remote daemons get a bounded health retry during startup.

- **AI Chat sent messages** — A sent prompt appears once when ego streams it back in chunks, including replies chosen from suggestion buttons.

- **AI Chat controls** — Pause, Resume, Compact and New use accessible icon buttons in one row, with a shortened model name that yields space to the actions.

- **MCP tool search** — `search_tools` again ranks `repo` first for GitHub PR and CI queries; its description names the PR/CI summary that `repo action=status` returns.

- **AI Chat transcript** — Message Copy appears on hover or keyboard focus, and a trailing inline `suggest:` token becomes reply buttons.

- **MCP bridge session cleanup** — A bridge closes its upstream protocol session when its stdio client exits. The server removes stale sessions left by abrupt bridge exits when the same peer reconnects, while preserving active sibling bridges. Failed bridge reconnects pause between attempts.

- **AI Chat session settings** — The control bar shows the current model and mode. A settings dialog labels every option ego offers and reports rejected changes.

- **AI Chat parity** — Select or copy transcript text and code, open web and file links through the terminal's handlers, and keep parallel ACP conversations in chat tabs. Finished tool calls stop pulsing even when ego sends no final tool update.

- **AI Chat permission alerts** — A pending ego permission or form request can alert a subscribed phone when the desktop is away. The notification opens the matching mobile conversation; answered requests and ordinary activity do not alert, and each conversation has a 30-second limit.

- **AI Chat failed turns** — ACP prompt diagnostics and empty replies appear in the transcript; small choice requests use direct buttons, and untitled conversations have readable picker labels.

- **Push-to-talk hallucination filter** — A bare “Grazie a tutti.” is discarded, while dictated instructions containing those words are retained.

- **Managed child questions** — Separate questions remain in the parent's inbox after earlier answers and later state updates.

- **Browser agent resume verification** — HTTP verification carries the agent PID and saved profile environment, so sessions under a non-default Claude, Codex or Gemini profile are checked in the correct store.

- **Codex approval cancellation** — Esc clears a canceled approval's waiting badge when the composer returns, without clearing a later question.

- **Agent menu run-config environment** — Active-terminal and sidebar launches apply the selected run config's environment only to the launched agent. Quoted and Unicode values stay intact, and menu overrides cannot replace TUIC peer identity variables, including differently cased names on Windows.

- **AI Chat session details** — Live ACP titles update the header and conversation picker. A footer shows context-window use and reported cost.

- **Agent menu run-config environment** — Active-terminal and sidebar launches apply the selected run config's environment only to the launched agent. Quoted values stay intact, and menu overrides cannot replace TUIC peer identity variables.

- **AI Chat image paste** — Pasting a supported image stages a removable preview and sends its bytes as an ACP image block when the agent advertises image prompts. Oversized or unsupported images show a refusal before upload.

- **AI Chat conversation recovery** — The last selected ego conversation reopens after a restart. The picker shows previous conversations by title and latest activity, then loads their history once.

- **Managed Codex wrapper trust** — Codex run configs that forward launcher arguments receive the same launch-only workspace trust override as direct Codex spawns.

- **Missing worktree cleanup** — A missing checkout's registered Git state can be confirmed and pruned from desktop or HTTP. Submodule refs are preserved first; lock override remains a separate confirmation.

- **Safer Markdown and WebView links** — Markdown file links with `:line:column` open at the requested line, paths traversing into macOS protected home folders are not probed, and WebView recovery saves only app-origin URLs.

- **Headless MCP voice binding** — `tuic-remote` refuses voice calls from connections without a live terminal before reporting that audio is unavailable.

- **Agent inbox FIFO and paging** — A full inbox accepts new mail by evicting its oldest message and reports unread losses in `missed_count`. Limited reads return oldest unread mail with `has_more`; an omitted limit returns all retained fresh mail.

- **MCP agent spawn configuration** — Spawn accepts per-child environment variables, with TUIC peer identity protected. Run configs can set an optional model that the spawn call overrides; legacy `--model` arguments keep their existing conflict behavior.

- **Faster desktop startup** — AI Chat and its markdown renderer load when the conversation opens, keeping the initial desktop bundle within its gzip budget.

- **Stable dev window** — Vite ignores tooling files in `.tmp/` at every directory depth, so temporary HTML cannot reload a live window.

- **Remote agent configuration errors** — Failed remote config loads identify the connection and endpoint, remain retryable, and cannot overwrite the remote file with an empty fallback.

- **WebView reload cleanup** — Reloading a desktop document releases its own terminal grid channels, delivery gates and plugin output watcher sets before the new document registers. Other windows keep their live subscriptions.

- **Urgent agent mail** — `agent send` now distinguishes default `normal` mail from `urgency="urgent"`. Urgent mail keeps its body in the inbox and submits a payload-free notice to a busy Claude Code or Codex recipient when the composer is safe. The CLI presents it at the next tool boundary without interrupting the current tool. The sender gets a receipt for the notice write or a queued-fallback reason; repeated urgent mail from one sender uses one unread notice until the inbox is read.

- **Claude activity status** — An idle Claude session no longer stays marked Working indefinitely when a mail wake reaches its detailed transcript view without starting a turn. Shell state changes now have a consistent trace.

- **Push-to-talk release and long audio** — Fn release stops microphone capture even while the WebView is busy, focus loss recovers missed key-up events, and slow streaming passes no longer discard the first 30 seconds of pending audio without a warning. File logs retain timing and length diagnostics without transcript text.

- **Activity Dashboard window size** — An oversized saved detached window now reopens at the compact 550×650 default instead of filling most of the display.

- **MCP tab repository ownership** — External file and HTML/URL tabs opened by an MCP caller now belong to that caller's registered repository, even when another repository is visible. Focused opens switch to the caller's repository; background opens keep the current view.

- **Terminal link positions** — File links with `:line:col` now place the editor cursor at the requested column. Clicking another position in an open file navigates the existing tab without discarding edits.

- **Terminal file line links** — Clicking a file path with `:line` now opens the built-in editor at that line, including for Markdown files.

- **External links in embedded previews** — Clicked web and mail links in HTML previews and inline plugin panels open outside the app. Cross-origin dashboards show a browser fallback toast when WebView navigation is blocked.

- **External Markdown review saves** — Inline tweak comments on Markdown files outside the active repository now write to the external file. A failed write shows an error and keeps the draft open for retry.

- **MCP file tab identity and visibility** — Native `tuic://open` and `tuic://edit` tabs use the MCP id: distinct ids coexist and repeating an id updates its tab. Tabs stay visible after selecting a terminal in a repository without an active workspace. Clicking an existing path outside registered repositories in terminal output opens it in the native viewer or editor.

- **MCP tab repository scope** — Unpinned native file, HTML and URL tabs opened through `ui action=tab` return when their opening repository is selected again. Pinned MCP tabs remain visible across repositories.

- **Open links from the code editor** — Cmd/Ctrl+click opens web URLs, Markdown links, local paths and directories. Paths can point outside a repository, and missing files show a toast. MCP `tuic://open` now previews external Markdown files as Markdown.

- **Code editor font zoom** — `Cmd/Ctrl` plus, minus and zero now zoom or reset the active editor tab with the same font range and step as a terminal.

- **Mobile session and feed repair** — Connected remote sessions open live output and close through their owning machine. Mobile Progress discovers projects with journal entries, and Activity loads saved events when opened.

- **Progress toast dismissal** — Tapping a toast closes it without switching repositories or terminals. The dedicated **Go to repo** button still navigates to the reporting workspace.
- **BREAKING: smaller native MCP tools** — `agent register/list_peers` now takes `path` instead of `project`; `repo worktree_lifecycle/worktree_remove` now takes `branch` instead of `workspace_id`. Removed `agent detect/stats/metrics`, `session process_stats`, and `repo prs/issues/close_issue/reopen_issue/ci_logs` in favor of their single-call HTTP routes. Each removed action returns an error naming its replacement. This saves tool tokens by keeping one name per concept and removing duplicate actions from schemas and output choices. The `worktree_create` response still carries `workspace_id` alongside `branch` for HTTP parity. `tuic agent list-peers --path` and `tuic repo worktree-remove <path> <branch>` use the new names.

- **Remote repository terminals and browsing** — New remote terminals attach to their owning machine's stream and show launch failures in the pane or stream failures in a persistent toast. Remote PTY creation rejects a missing working directory instead of registering an empty terminal. The remote folder picker starts at the host's reported home directory and shows readable permission errors while keeping path entry available.

- **Subagent attribution** — The Activity Dashboard uses the same robot marker as the sidebar. Its tooltip names a live parent even when the parent's TUIC identity differs from its PTY ID.

- **Compact Activity Dashboard** — The inline overlay matches the detached window's compact width and remains inside narrow main windows.

- **Ask from the phone** — A managed agent's explicit blocked question reaches the mobile PWA by Web Push when the desktop is away, including a focused Mac idle for two minutes. The notification opens that session; a typed answer uses the same atomic submission receipt as the coordinator. Disabled push diagnostics no longer claim a delivery.

- **Progress stays accessible** — The toolbar bell always includes Terminal Progress, even after all updates are read. Open it with `Cmd/Ctrl+Shift+P` or from the command palette; the unread badge keeps counting new entries.

- **File browser opens in tree view** — New installations and preferences without a saved view mode show the directory tree by default. An explicitly saved flat list remains selected.
- **Markdown links stay in the app** — File and directory links in rendered Markdown open the appropriate TUICommander view; heading and line targets work, missing files show a toast, and unsupported URL schemes cannot navigate the WebView.
- **Markdown links stay in the app** — File and directory links in rendered Markdown open the appropriate TUICommander view; heading and line targets work, missing files show a toast, and unsupported URL schemes cannot navigate the WebView. Local symlinks and parent paths work, UNC links are refused, and internal preview/plugin frames remain available.
- **MCP bridge startup guard** — A TUICommander executable without its `tuic-bridge` sidecar no longer edits agent MCP configs. An explicit install cannot replace an existing bridge entry with an unresolved bare command; a missing entry may still use that fallback with a warning that lists searched paths.
- **MCP bridge startup guard** — Startup requires a non-empty, executable sidecar from a stable app location. A dev or stray build keeps an existing working absolute MCP command, while a missing command can be repaired. Config edits preserve permissions, symlinks, comments and user-added entry fields; an unresolved bare command cannot replace an existing entry.
- **MCP bridge startup guard** — Startup requires a non-empty, executable sidecar from a stable app location. A dev or stray build keeps working absolute commands, custom wrappers and HTTP entries; only a bare bridge name or broken absolute path is repaired. Config edits preserve permissions, symlinks, comments and user-added entry fields. Goose edits retain the file's indentation and verify that other extensions are unchanged.

- **CircleCI CI Auto-Heal** — Failed CircleCI checks can now contribute bounded, sanitised logs when a token is configured. Token removal is explicit, and remote transports use the same protected backend path.

### Added

- **Update and restart remote machines** — Direct and SSH connections compare
  the daemon's running build with the binary the desktop can deploy. A
  per-connection action confirms live session loss, updates the daemon, and
  verifies the new build after restart. Direct uploads require the daemon
  session token and validate size, target and SHA-256.


- **Managed agents start in new folders** — Claude Code and direct Codex children spawned by another agent pass their workspace trust question by default. A per-agent expert setting turns this off; user-opened terminals keep their usual trust behavior.
- **Configurable PTY capture directory** — `TUIC_CAPTURE_DIR` sends diagnostic `.tcap` files to an absolute path selected at process start. Relative paths reject capture activation.

- **Editor line wrapping** — Text and Markdown files open wrapped, while code opens unwrapped. Toggle it from the editor header or with `Alt+Z`; each file kind remembers its setting.

- **Settings expert mode** — An **Expert** switch in the Settings header shows or hides expert settings: settings whose default is correct for almost everyone. In basic mode an expert setting is hidden only while it has its default value, so a setting you changed is never hidden. A search result for an expert setting shows an **Expert** badge and opens it without changing the switch. The switch position is kept between restarts.
- **Native plans and stories** — A config-directory SQLite store tracks plans, criteria, dependencies, revisions, manual claims, and guarded review transitions. Typed manual actions are available through desktop IPC, authenticated HTTP, managed-session MCP, and `tuic story`.
- **Manual Plans and Stories dialog** — A project toolbar action opens native plans and stories on desktop or in the browser. Users can create records, start a Ready story without a terminal claim, update criteria, manage dependencies, and review or block work.
- **Versioned workflow definitions** — Project-scoped plan and story graphs can be saved as drafts, validated, and published as immutable revisions. The built-in `Resolve plan` graph pins a version of `Story delivery`. Running and editing graphs visually are later parts of this feature.
- **Durable workflow run foundation** — Project-scoped plan runs now store ordered events, pinned revisions, attempt and effect projections, bounded retries, and recovery state in SQLite. Desktop and HTTP clients can page history and send explicit commands; automatic scheduling and graph UI are still in progress.
- **Plan run timeline** — Run history in Plans and Stories lists durable runs for the selected plan and pages their ordered events. Desktop and browser clients refresh on workflow run changes.
- **Workflow review and input evidence** — Managed reviewer reports can carry criterion-indexed findings and an artifact digest without changing story status. A worker can request input, pausing the run until an operator records an idempotent answer and resumes it.
- **Workflow draft designer** — The Plans and Stories dialog now includes a draft editor for adding and connecting graph nodes, changing agent prompts and loop limits, and publishing a validated revision. The node palette supports drag and drop and click controls.
- **Conservative workflow dispatch** — Explicit story attempts now serialize overlapping or unknown file scopes, cap parallel workers, and record a distinct registered worktree per story before spawn. Dependent stories stay held until an integration receipt exists.
- **Checked story integration** — TUIC records pinned command receipts from assigned story worktrees and verifies an operator-created merge with post-integration checks before releasing dependents. A changed canonical ref or tree invalidates the receipt. HTTP story actions without a managed session record local API provenance and cannot approve a story.
- **Canonical recertification** — After an unrelated commit advances the canonical branch, TUIC can rerun pinned checks at its clean tip and record a durable receipt for previously integrated stories. Parallel workers retain valid reports even when another worker pauses the run for input.
- **Story transition provenance** — Native story transitions now keep a durable actor and revision history. Managed MCP agents cannot approve stories; the user action remains the approval path.
- **Workflow closure policy** — Definitions default to human approval. Automatic closure can be drafted but publication rejects it until TUIC can verify the required evidence.
- **Pinned workflow checks** — Published definitions now retain a bounded set of direct-executable checks. The runtime receipt path will use these pinned commands rather than an agent's test claim.

- **Progress Flow view** — The Progress dialog has a **List | Flow** toggle. Flow draws the journal as a sequence diagram: one column per terminal and per Claude subagent, delegations and peer messages as arrows, each child's done or blocked as its return arrow, and intents as notes. TUICommander now records `delegated` at `agent action=spawn` and `message` at `agent action=send`, redacted and capped at 500 characters. Click a label to read all of it; a subagent's full prompt or report is fetched on demand. An existing journal is migrated on first open.
- **Hold `+` to open an agent** — A long press on the tab bar's `+` lists the enabled agents and their run configs; picking one opens a new tab in the active branch with that agent already started. A click still opens a plain terminal.
- **Design Mode for agent terminals** — Open a dedicated Chrome window on a repository's development server, click an element to capture its selector, styles, the design tokens its rules use, source location and optional screenshot, then edit the inserted agent draft before sending. Repeated grabs accumulate; the same Chrome window can be rebound to another agent terminal in the repository. The development-server URL is a local per-repository setting.
- **SSH remote machines deploy and install themselves** — “Deploy on connect” resolves and caches the matching `tuic-remote` release, copies it only when its hash changes, launches it on loopback with a vault-backed pairing token, and reconnects through the SSH tunnel. A configurable idle lifetime preserves sessions across short disconnects. Linux systemd user units and macOS launchd agents can be installed or removed from Remote Machines, while the SSH host picker reports shell, no-shell, authentication-failed and unreachable hosts without duplicates.
- **Agent activity in the sidebar** — The optional nested-terminal view now expands every occupied branch into one compact card. Agent rows reuse Activity Dashboard state and show the agent icon, current intent/task, last-update age, and attention state; plain terminals remain selectable in the same card, including single-session branches.
- **Agent toasts can jump to their repository** — A toast from another registered repository shows a keyboard-reachable “Go to repo” action. It selects that repository and focuses the originating session when the session is still open; local and stale-repository toasts do not show the action. (#835-314c)
- **Hands-free voice conversation** — Settings > Voice binds one terminal, keeps the microphone open and sends each utterance by itself. An optional activation phrase gates which speech is sent and is stripped before sending; a configurable hold-back shows a finished utterance before it goes, so one you did not mean can be stopped. Speech is typed into the agent at once, even while it works — the way a line typed by hand is — and never waits in the Compose queue; an open permission dialog or text you are typing holds the turn until it is gone. The conversation ends by itself when the bound terminal closes, the owner disconnects or the capture device fails.
- **Spoken replies** — Agents answer out loud through local Kokoro synthesis, over IPC, HTTP and MCP alike. Runtime library and per-language voice bundles are downloaded and verified from Settings > Voice. Replies use the language of the conversation rather than defaulting to English, and the agent is told when it may answer out loud and when to go back to text.
- **Talking over a reply stops it** — Echo cancellation runs on the captured audio, so the reply coming out of the speaker neither interrupts itself nor becomes a turn, while the user's own speech stops playback and opens the next turn.
- **Voice progress reaches browser and remote clients** — Model and speech-asset download progress, and every state a spoken reply passes through, are pushed on the `/events` stream as well as to the desktop window. A browser tab no longer watches a download start and finish with nothing in between, and no longer polls to learn that a reply ended.
- **Hands-free earcons** — A short, quiet blip tells you a spoken turn reached the agent, and a softer one that the activation phrase dropped it, so you do not have to look at the screen. Works on the desktop and in a browser; only the device you are talking into plays it. A dictation setting turns them off.
- **Custom hands-free start notice** — Settings > Voice lets you replace the notice the agent reads when a hands-free conversation starts with your own instructions. Empty, or **Reset to default**, sends the built-in text.
- **Hands-free conversation from a browser tab** — A browser or PWA client uses its own microphone and speaker: Settings > Voice now opens outside the desktop app, and starting a conversation there is heard and answered where you are. The two sides never mix — arming from a laptop cannot open the microphone on the machine running TUICommander, and the desktop cannot take audio from a browser — and a client that has no audio connection is refused rather than quietly served the server's hardware. Closing the tab ends the conversation it held, leaving nothing queued and nothing speaking. The global dictation hotkey and the microphone-device list stay desktop-only.
- **Start hands-free from the Command Palette** — "Start hands-free conversation" binds the active terminal and opens the microphone; while it runs the entry reads "Stop hands-free conversation". No default shortcut; `toggle-hands-free` can be bound in Settings.
- **Leaner MCP responses for orchestrators** — `session action=output` no longer returns an empty input box and the status line or HUD below it on a tail read (`format=raw` still does). A permission dialog or a draft in the input box is never cut. `agent action=spawn` no longer repeats `monitor_with`, `status_with`, `wait_with`, `peer_monitor_with` and `peer_wait_with` call templates, which were most of each spawn response; `parent_session_id` still tells whether child-to-parent mail works.
- **MCP `session action=rename`** — An agent can set a tab's display name directly instead of relying on an `intent:` title or the desktop UI. Sticky by default (`is_custom` true), same as a manual rename, so a later OSC/intent title update does not overwrite it unless `is_custom=false` is passed.

### Changed

- **Safer worktree removal and consistent warm status** — Removal without force now keeps dirty worktrees and refuses in-progress Git operations. Branch deletion checks that the branch tip has not changed since inspection. Desktop IPC, HTTP, and MCP creation all report pending build-input warming; removal stops queued warming from recreating a deleted directory.
- **Settings navigation grouped by task** — Pages now sit under **Application**, **Workspace**, **AI**, **Integrations** and **Repositories**. **Services & MCP** is split into **MCP**, **Remote Access** and **Remote Machines**. Terminal fields from General and Appearance move to a new **Terminal** page; the ego executable and the provider list join on **AI Chat**, which replaces **AI Providers**; the shortcut editor is also a **Keyboard Shortcuts** page; **Dictation** is now **Voice**. Saved settings are unchanged.
- **Agent list on the `+` buttons** — Right-click or long press on the tab bar `+`, and a long press on a sidebar branch `+`, list the enabled agents and open a tab running the chosen one. Right-click on a sidebar branch `+` still opens the branch menu. Before, the sidebar `+` had no long press, and right-click on the tab bar `+` opened New Tab / Split, which stay on `Cmd+T`, `Cmd+\`, the terminal context menu and the Command Palette.
- **Sub-agent rows in the sidebar show an icon** — A nested sidebar row of an agent spawned by another one shows a small agent icon instead of `↳ <parent>`, which took up to 40% of the row. The parent's name is in the icon's tooltip.
- **Voices download straight from Kyutai** — Speaker embeddings now come from Kyutai's ungated `pocket-tts-without-voice-cloning` repository, pinned to one commit, instead of a copy on a TUICommander release. The files are byte-identical; nothing has to be re-hosted to offer another language.
- **Spoken replies in five more languages** — English, French, German, Portuguese and Spanish join Italian, each with the voice Kyutai lists as native to it. Every file is pinned to one upstream commit and a sha256, like Italian. French ships only as the larger 24-layer model (about 390 MB).
- **Progress by terminal** — Opens on the active PTY, lets you switch between PTYs in the same repository, and retains an **All repo** view. Older entries remain visible there without a guessed terminal assignment.
- **Bundled themes reviewed** — Commander, Darksun, Minimal Kiwi, Monokai, Tokyo Night, Solarized Dark, Ink and Paper separate the main area, sidebar, hover and selection more clearly and draw visible borders. Commander's chrome matches its terminal background and its accent is brighter; Minimal Kiwi's success green no longer equals its accent; Paper has readable terminal yellow and cyan. "Clean" is renamed **Ink** and "VS Code Light" **Paper** (theme keys unchanged, so saved settings keep working). VS Code Dark now follows VS Code's current default, Dark 2026 (darker, cooler greys), instead of the old Dark+ palette. Commander is now the default and the fallback for an unknown theme. The toolbar wordmark, sidebar tab selection and repository name colors follow the theme, so they stay readable on Paper.

### Removed

- Removed the `/agents/map` subagent map page and its `/agents/map/data` and `/agents/map/prompt` routes. The Progress dialog's Flow view replaces them.
- Removed the Deep Black and Delicate One bundled themes: near-duplicates of Ink and of the neutral greys. An existing install keeps the files it already has in its themes folder.
- Retired the external Voice plugin that read terminal text through Web Speech synthesis. Native Kokoro voice conversation is planned separately in Dictation.

### Fixed

- Agent command injection uses the longer Codex-safe Enter gap when the agent type is still unknown. MCP `session input` also separates text and Enter for Claude and other identified agents.
- Worktree cleanup recognizes a merged GitHub PR whose head contains the local tip after a squash merge, and recognizes branches already contained in the checked-out integration branch. Warm copies retain ignored build directories while restoring owner write permission in the new worktree so sealed source caches do not block removal. MCP `repo branch_delete` safely deletes an integrated local branch without a worktree.
- Dictation auto-send and other injected prompts now submit in Codex after its paste-burst Enter suppression window, in both the frontend and MCP session paths.
- **Agent native scrollback** — TUICommander launches Claude, Codex, Grok and supported OpenCode versions without alternate-screen rendering by default, including resumed and orchestrated sessions. A one-time warning names any agent that still enters alternate screen.
- **Agent native scrollback** — TUICommander launches Claude, Codex, Grok and supported OpenCode versions without alternate-screen rendering by default, including resumed and orchestrated sessions. One Rust policy also prepares commands typed into existing terminals. Capability checks have a deadline and warn on failures; a one-time warning names any agent that still enters alternate screen.
- **Native scrollback for agents typed in shells** — New TUIC zsh, bash and fish shells apply supported Codex, Grok and OpenCode screen flags to manually entered commands. Settings → Agents has a default-on per-agent control for new launches; explicit CLI flags and non-interactive subcommands retain their behavior.
- **Progress intents survive narrow terminals** — Hard-wrapped intent text now keeps its complete title across several rows. Adjacent indented prose stays separate, long intents keep their tab-title event while the journal stores a redacted 500-character version, and ordinary repaints avoid unnecessary grid reads.

- **Side panels follow an agent click to another repository** — A click on an agent row under another repository in the sidebar now moves the Notes, Git and Files panels to that repository, as a click on its branch row does. Opening another repository's settings no longer moves those panels there.
- **No duplicated agent rows after a WebView reload** — A surviving agent tab is re-adopted with the intent and prompt the backend holds, so its Context bar is there before the terminal measures. Before, the bar appeared later, the pane shrank, and Claude's repaint for the transient taller height was pushed into scrollback a second time.
- Mobile terminal output rejoins agent prose wrapped at the desktop PTY width before wrapping it for the phone, while preserving lists and tables.
- **Progress records worktree terminals** — An `intent:` from a terminal running in a managed worktree outside the repository root was dropped because the directory matched no registered repository. It now resolves to its workspace and is recorded against the parent project.
- **Multi-line teammate prompts join their spawn** — A teammate subagent was matched to its spawn by comparing the prompt with the raw JSON line, where every newline is escaped, so any multi-line prompt never matched.
- Dictation auto-send submits long transcriptions to Claude Code again. Claude
  treated a long text as a paste, stripped the Ctrl-U prefix inside it as an
  invisible character and refused the following Enter. Agents now receive
  Ctrl-U, the text and Enter as separate, time-spaced writes — both from the
  frontend and from MCP `session submit` and peer delivery.
- Restore captured agent intent and the last substantial prompt from backend
  session snapshots, including reconnects and state updates. An idle Grok tab
  no longer needs another parsed output event to recover its Context bar.

- Keep a new terminal drag selection visible while output arrives after an
  earlier selection was copied. The previous copied text no longer invalidates
  the range currently being dragged.

- Keep terminal selections attached to retained text when output rotates a full
  scrollback buffer. Copying uses the displayed history snapshot so further
  output cannot silently substitute a different row.

- Use the newest visibility observation after terminal resize. A queued hidden
  observation followed by a visible one must not leave the canvas ignoring live
  frames while its backend rows remain intact.

- Preserve terminal combining marks through text extraction, live and scrollback
  grid frames, canvas painting, copy and search. Marks arriving after their base
  letter now trigger a repaint; Unicode string offsets no longer displace links
  or buffer-search highlights.

- Dictation no longer drops a live transcription window merely because its final second is quiet. Whole-window speech gates remain active.

- **A repainting agent no longer grows the scrollback it never printed.** `CSI M` (delete
  lines) and `CSI S` (scroll up) pushed the rows they removed into history, so a TUI that
  redraws with them added scrollback the agent never wrote. Lines now enter history only
  when they leave the bottom of the screen; a linefeed past the bottom margin is unchanged.
  (#834-1878)
- **`tuic agent send` no longer reports a successful delivery as refused.** The
  registry removed its ambiguous `accepted` field when `delivered` and
  `delivery_path` became authoritative, but the CLI still required that removed
  field after sending. A message could wake its recipient and then make the
  sender exit with “Registry did not accept the message”, inviting a duplicate
  retry. The CLI now validates the current three-field delivery report and
  accepts both surfaced and honestly buffered inbox-only routes.

- **Child results wake a ready orchestrator even while background work remains.**
  A real background descendant keeps the derived task state `working`, so the
  mail router rejected a child `RESULT` even when the shell was
  idle and the empty composer was confirmed ready. The router now reuses the
  stricter composer-safety gate for that one state: the parent receives only the
  payload-free inbox notice, while busy turns, questions, and partial drafts stay
  untouched and the authoritative result remains exactly once in the inbox.

- **The AI Chat panel reaches TUICommander's tools on a default install.** The one
  MCP server an ego session is given was an HTTP URL built from the TCP port this
  process bound, and that port only exists while the TCP listener is up — which
  happens only when Remote Access is enabled, and Remote Access is off by default.
  So an ordinary install handed ego no server at all: a session that opened
  normally and could reach nothing, with no error to say why. It now runs over the
  `tuic-bridge` sidecar on stdio, the same adapter every terminal agent already
  uses to reach `mcp.sock` — a socket that binds unconditionally. Nothing new
  listens, and turning Remote Access on or off no longer changes what ego can do.
  A named `TUIC_APP_INSTANCE` travels with the entry rather than relying on being
  inherited twice, so a test instance cannot end up driving the default one's
  repositories.

### Added

- **SQLite databases open in a self-contained plugin viewer.** The registry's
  SQLite Viewer claims `.db`, `.sqlite`, `.sqlite3`, `.db3`, and `.s3db` files
  and shows tables, views, schema, indexes, per-column filters, sortable native
  SQL pages, configurable page sizes, bounded read-only SQL, CSV copy, and visual
  query plans. Tables with primary keys support explicit inline editing and an
  atomic save back to the source file; encrypted/SQLCipher input gets a clear
  unsupported-format diagnostic.
  Its pinned sql.js JavaScript and WebAssembly ship inside the plugin: there is
  no CDN request, system SQLite dependency, or SQLite-specific host API. The
  engine and database live in the panel iframe and are released with the tab.
  Generic plugin APIs now accept optional bounded binary read/write budgets, can
  transfer `ArrayBuffer` ownership through `PanelHandle.send`, and serve `.wasm`
  assets with the standard MIME type. Existing binary reads retain their 10 MiB
  default; the host ceiling for an explicit budget is 512 MiB.

- **PR review, changelog generation and the improvement scan are back, on ego.**
  All three went with the embedded engine and all three now run as one unattended
  ego turn: the whole input goes inline, ego is offered no TUICommander tools, and
  every permission request is refused. TUICommander stores no API key and makes no
  provider HTTP call for any of them, and none of them names a model — which model
  runs is ego's own configuration.
  - **PR review** reads the PR diff (at most 30 files, 300 lines each, with the
    truncation stated in the prompt so nothing is silently dropped) and returns
    findings with severity, path, line and confidence. The confidence gate runs in
    Rust before the result is built — default `0.7`, `TUIC_REVIEW_CONFIDENCE_THRESHOLD`
    overrides it — so a finding that reaches the popover has already passed it.
    Selected findings still post to GitHub as inline review comments, and GitHub's
    rule that a comment needs a concrete line still decides which are selectable.
  - **Changelog generation** produces the markdown plus the structured JSON split
    over the merged PRs since a tag. Ego answering in prose only is a valid answer,
    not an error: the JSON half is `null` and the markdown still renders.
  - **The improvement scan** returns at most five proposals for `refactor`,
    `testing` or `perf`, and each one is promoted to a GitHub issue only by an
    explicit click. Nothing files an issue by itself.
  The three do not fail quietly. An ego that is not configured, not reachable, or
  that ends a turn without answering produces ego's own sentence — "no ego
  executable is configured", "ego asked to use 2 tools that an unattended turn
  cannot grant, and produced no answer" — where the deleted version would have
  shown an empty result that reads as "nothing to report".
  The four surfaces are reachable over HTTP as well (`POST /repo/pr-review`,
  `GET /repo/changelog`, `POST /repo/improvement-scan`,
  `POST /repo/create-issue-from-proposal`) and are deliberately not desktop-gated:
  ego is reached over ACP, so `tuic-remote` serves them too.
- **The terminal stream is compressed over a remote connection.** HTTP bodies
  were already compressed; the WebSocket carrying the terminal was not, and it
  is the traffic that matters — an agent repainting a full screen sends the grid
  many times a second. A remote client now opens the stream with
  `?compress=deflate` and every frame arrives raw-deflated, with a one-byte tag
  naming its encoding so a frame that does not shrink is sent as it is rather
  than sent larger. Measured on a replay of a real Codex session: 2.2 MB of grid
  frames become 127 KB, and a full-screen repaint costs 0.234 ms to compress
  against a 16 ms frame budget. A client that does not ask gets exactly the old
  framing.
- **An SSH tunnel compresses its channel.** `Compression=yes`, on by default and
  switchable per profile. It is also why the WebSocket refuses to deflate for a
  loopback peer: a tunnelled client reaches the daemon through the local ssh
  process, so its frames would otherwise be compressed twice.
- **Smart Prompts run in `api` mode again, on ego.** The mode used to call a
  provider directly over HTTP from TUICommander; that client, its registry and
  its keyring entry went with the embedded engine, and none of them came back.
  An `api` prompt now runs one unattended ego turn — launch the configured ego
  in the active directory, one session, one prompt, shut down — and the turn's
  final text goes to the prompt's output target. Nothing streams, because a
  Smart Prompt runs with no panel open. A `headless` prompt whose agent resolves
  to `api` takes the same one path. The model is ego's default, set in Settings
  → AI Providers; with no ego binary named the mode refuses and says to name one
  under Settings → General first.
- **An unattended turn gets no TUICommander tools, and cannot be left waiting.**
  Every attended ego session is handed TUICommander's own MCP server, which is
  how ego reaches terminals and repositories. A turn with nobody at the keyboard
  to approve a single call is opened with **no MCP server at all**, and every
  permission request and elicitation ego makes is refused the instant it
  arrives — a seat nobody takes is a turn that never ends. How many were refused
  comes back with the answer, so a prompt that returned nothing because the
  model reached for a tool says that, instead of looking like an empty reply.
  The turn is abandoned after 300 seconds, a server-side limit no request can
  raise.

### Fixed

- **Notification sounds no longer overlap during a burst.** The anti-spam gate
  used to be tracked separately for each tone, while every accepted tone opened
  its own native playback thread. A completion, question, and attention event
  arriving together could therefore all play at once. Normal notifications now
  share one 500 ms playback gate; explicit Settings test clicks still bypass it.
  Each native tone also ends on an exact zero-amplitude sample and drains 100 ms
  of silence before closing the output stream, removing the crackle that could
  follow an otherwise clean chime.

- **A dependency's `log` output is no longer presented as a TUICommander
  error.** `tracing_log` gives every record it bridges from the `log` facade the
  target `"log"`, so the ring buffer filed them under that literal and the
  audience fell through to `user` — the default tab of the error log, and the
  only audience that moves the unseen-error badge. A TLS-intercepting middlebox
  on the network was therefore reported as an unbroken flood of app errors from
  `rustls_platform_verifier`, one per registry retry. Bridged records now carry
  their real module as the source and the `diagnostic` audience; nothing is
  dropped, and the line that is actionable ("Upstream '<x>' request failed") is
  ours and unaffected.

### Changed

- **One MCP tool family, not two.** The 13 `ai_terminal_*` tools and the
  `ai_terminal_mcp_enabled` flag are gone. They overlapped `session`/`repo`/
  `agent` without being equivalent, so a model paid for both catalogues on every
  turn and had to guess which to call, and six of them needed a filesystem
  sandbox only the embedded agent loop creates — they refused every external
  caller before dispatch. External clients drive terminals through `session`
  (`action=submit|input|output|status|wait`). **Secret redaction moved with
  them**: `session action=output` now redacts, which it never did, so every MCP
  client — Claude Code included — gains a protection instead of losing one. The
  mandatory native write confirmation was dropped on purpose: no remote client
  can answer a blocking OS dialog, and `ui action=confirm` is the gate that any
  client can answer.

- **TUICommander's `/mcp` now answers the 2026-07-28 stateless lifecycle**
  (`server/discover`, SEP-2549) beside the legacy `initialize`. A client that
  pins the modern revision and has no fallback used to get `-32601` and reach
  zero TUIC tools. Stateless callers send their identity in `_meta` on every
  request, which is what keeps tool collapsing working without a handshake.

- **Usage dashboards now use provider-owned APIs.** Codex usage comes through
  the documented local App Server instead of reading OAuth credentials and
  calling private ChatGPT endpoints. Grok gains a native usage ticker and
  dashboard backed by its `_x.ai/billing` ACP extension. Gemini remains
  terminal-detection only because its CLI exposes no stable account quota API;
  missing provider data is never presented as zero usage.

- **Sidebar worktree warnings now explain themselves.** Hovering or keyboard-focusing
  a `Dirty` or `Unknown` lifecycle badge shows what the state means, what it does
  to removal safety, and the underlying inspection error when one is available.

- **Project Progress is one journal, one database and a dialog.** The feature
  shipped in 1.7.7 asked agents to classify outcomes into five kinds, kept a
  SQLite store inside every project, and spread the result over a sidebar panel
  with five views. Across 39 repositories it recorded nothing. It now records
  two things an agent can report — `done` and `blocked` — plus the `intent:`
  marker TUICommander already parses and used to discard, so a three-hour
  session that never called the tool still answers "where did it get to?".
  History lives in a single append-only database in the configuration
  directory, so a deleted workspace or a temporary clone cannot take it away,
  and the reporting duty is stated in the connection instructions instead of
  being left to a tool description nobody reads. The panel is now a dialog
  showing one project's list newest-first, with a line marking where the last
  visit ended that stays put while you read. Collection has an off switch,
  globally and per agent.

### Removed

- **The `progress.md` export, and eight of the nine `repo progress_*` actions.**
  Export, status, pause, resume, clear, correct, acknowledge and read-cursor
  control are gone; `progress_list` remains. Existing per-project
  `.tuic/progress.sqlite3` databases are not migrated — the old history was
  effectively empty, and nothing reads those files any more. Delete them at
  leisure.

### Fixed

- **The Progress journal no longer fills with the same intent.** A repeat of a
  project's newest intent — same text, same agent — now returns that entry
  instead of adding a row. The changed-row parser handed the same `intent:`
  line back on every repaint it survived (one intent landed 17 times in 8
  seconds), and 596 of 764 recorded entries were byte-identical to the one
  before them. Agent reports (`done`, `blocked`) are still never collapsed.

- **The delete icon in the Progress dialog hides again when the pointer leaves
  the row.** It followed the tab bar's hover-reveal pattern only halfway: the
  opacity fade let WKWebView leave the icon visible on every row the pointer
  had crossed. The fade is gone and the reveal is gated on `hover: hover`.

- **A closed plan tab no longer comes back when you re-enter the repository.** The
  Plan Tracker plugin re-opened the plan named in `.claude/active-plan.json` on
  every repository switch, pinned, even after the user had closed it. It now
  opens each plan once per app run, like the plans it discovers from terminal
  output and from the `plans/` watcher.

- **`make dev` no longer starts on the isolated test configuration.** The
  default that points `make test` at its own `instances/tuic-test/` namespace
  was written as a bare `TUIC_APP_INSTANCE?=tuic-test`, which is a *global*
  make variable however far down the file it sits, so `make dev` expanded it
  too and the daily driver came up against an empty configuration directory:
  every repository appeared to have vanished. No data was lost — the production
  `config.json` and `repositories.json` were never opened — but the fix had
  already been written once and lost, and the fright arrived a second time.
  The assignment is now scoped to the `test` target, both documented override
  forms still win, and `make dev` prints the configuration directory it is
  starting on, because an inherited `TUIC_APP_INSTANCE` still beats the
  Makefile and no check can see a developer's shell.

  A guard in `make check` asks make what it actually expands rather than
  trusting how the line reads, and `pre-commit` runs it whenever the `Makefile`
  is staged — `make check` is the once-per-batch target, and this repository
  pushes straight to `main`, so a bad edit used to land before anything looked
  at it. The guard reports a distinguishable failure when `make -n` errors or
  when a recipe stops printing the variable at all; previously both cases
  produced the empty string, which is also the correct answer for `make dev`,
  so a broken `Makefile` passed the check and printed its tick.

## [1.7.7] - 2026-09-16

### Added

- **Settings → AI Providers shows what `ego` is configured with, and sets the
  model it starts from.** It replaces the provider registry deleted with the
  embedded engine, and it is not a registry: every value is read and written by
  running ego — `config ls`, `models`, `doctor`, and `config set model` — so
  TUICommander stores no API key, puts nothing in the OS keyring, and makes no
  provider HTTP call. The one call that reaches a provider is
  `ego models --refresh`, which ego makes, and only when you press Refresh;
  opening the tab makes none. Credential state comes from `ego doctor` and keeps
  its distinctions: a store that could not be *read* is not an empty store, and
  an expired credential says ego renews it on its next run instead of sending
  you to log in again. `model` is the only writable key and it is its own
  operation rather than a key/value pair, so no caller over IPC or HTTP can
  reach `sandbox` or `permissions.judge`. When ego is not configured the tab
  names the field to fill instead of rendering an empty list, and when an ego
  command fails it shows the command, the exit code and what ego printed, word
  for word. Offered while Experimental Features is on, beside the AI Chat panel
  it configures.
- **Project Progress answers "what changed since I last looked?"** Agents
  record outcomes — capabilities, decisions, discoveries, blockers, completed
  objectives — through one compact MCP `progress` call that persists the event
  and shows its toast together; starting or finishing an agent task is
  deliberately not an outcome. History belongs to the project, in a SQLite
  store at its own root that local Git excludes cover, so closing a session or
  deleting a temporary workspace never takes it away. A dedicated panel, opened
  from the command palette or the notification bell, shows all projects or one:
  since last visit, today, blockers, completed, and full history, with
  provenance on demand. Collection can be paused and resumed per project,
  events can be deleted, corrected, merged or re-grouped, and clearing a
  project pauses it in the same transaction. There is no inference job and no
  background model: grouping is exact and every sentence in the panel is one an
  agent wrote. The optional fuller reporting prompt, and the measured
  comparison against the short default, are in the user guide.
- **Progress exports a readable `progress.md` when you ask it to.** One
  deterministic snapshot of the owning project — revision, UTC time, workstream
  states, open blockers and dated history, with source metadata off by default —
  is previewed in the panel before anything is written. Writing succeeds only
  while that same snapshot is current, replacing an existing file needs the
  exact content the preview returned, and a hand edit, a removal, a symlink or a
  directory at the target is refused instead of overwritten. The write is atomic
  and touches nothing else: no Git staging, no change to collection or read
  state, no scheduled re-export, and the Markdown is never read back in.

### Changed

- **Workspace creation now has one mechanism: linked Git worktrees.** The
  experimental whole-repository copy-on-write workspace path, its adopt,
  publish, recovery, and lifecycle projections, and the `mode`/`dirty` fields
  on Tauri, HTTP, and MCP creation requests have been removed. Linked
  worktrees still arrive warm: Git-ignored directories are copied with the
  existing copy-on-write primitive when the filesystem supports it. Parent
  tracked changes are deliberately not inherited.

- **Project Progress can now export a safe, versionable `progress.md`.** The
  project panel previews one deterministic backend snapshot and writes it only
  while both the database snapshot and existing-file content still match.
  Exports target the owning project root, optionally include bounded provenance,
  refuse symlinks/directories, and use atomic replacement without changing
  Progress state or Git.

- **Claude and Codex status signals are launch-scoped and on by default.** TUIC adds a private Claude settings file or Codex notify adapter only to processes launched inside TUIC. Explicit CLI overrides win, Codex still calls the user's existing notify command, and each agent has an independent off switch. Global hook installation remains explicit for agents without a launch-scoped route.

- **Plan Tracker and Stories Ticker are now external plugins.** The former
  compiled built-ins are installed once as ordinary plugin packages during the
  upgrade, preserving existing behavior while making both independently
  uninstallable and updateable from the plugin catalog. Plan file opening now
  uses a capability-gated background-tab host API instead of importing app
  stores directly.

### Fixed

- **Plugins can no longer read legacy MCP-upstream credentials.** The
  `credentials:read` guard now rejects the historical `tuicommander-mcp`
  keychain service through the same constant used by credential migration,
  while plugin-owned service names remain readable.

- **The bundled BM25 search index no longer depends on the unmaintained
  `fxhash` crate (RUSTSEC-2025-0057).** The ~90 lines it used (`FxHasher`,
  `FxHasher32`, `FxHasher64` and the `hash`/`hash32`/`hash64` free functions)
  are now vendored directly into the `bm25` fork, with the upstream
  `byteorder` reads replaced by `u32`/`u64::from_ne_bytes`. The algorithm is
  bit-identical — pinned by a golden-vector test against the real crate and a
  fixture snapshot captured before the change — so every existing
  content-index snapshot on disk still loads and searches exactly as before.

- **A hydrated repository with a missing or blank display name no longer crashes
  the whole app.** `normalizeLoadedRepo` now sanitizes it to a deterministic
  path-derived fallback at the persistence boundary (load and remote adoption
  alike), and the Command Palette's own sort no longer trusts a malformed
  action's label — it falls back to the action's id and logs the offender once
  via `appLogger` instead of spamming on every keystroke's re-sort.
- **The crash screen now copies the complete diagnostic, and desktop/mobile
  share one implementation.** A shared `CrashScreen` component (used by both
  `ErrorBoundary`s) copies the exact rendered message plus the full stack
  through `src/utils/clipboard.ts`, shows a visible "Copied"/"Copy failed"
  state, and logs a copy failure via `appLogger` rather than `console`. Reload
  is unchanged.
- **A stale-temp shell repository (empty, non-git, path gone, created under a
  temp root) is now classified server-side and quarantined from the sidebar
  instead of remaining a permanent ghost row.** The classifier
  (`config.rs`) requires ALL of: filesystem metadata proves the local path is
  absent (`NotFound`; permission and other I/O errors preserve it), it falls
  under a recognized temp root, `isGitRepo` is explicitly `false`, there is
  exactly one shell-only workspace with no terminals/saved terminals/commit or
  parent state, and no user metadata — any one mismatch leaves the row untouched, so
  a legitimate offline/unmounted repository is never touched. Repair is
  user-explicit only: the sidebar surfaces a count and a list of exact
  candidates, and confirming writes a timestamped backup before removing
  precisely those rows in one transactional write (refusing the whole request
  if any named row no longer classifies as stale). No implicit or global
  deletion.
- **A debug/test desktop launch can now get its own isolated config directory.**
  `TUIC_APP_INSTANCE=<id>` (read once at the very top of `run()`) reuses the
  same named-instance machinery `tuic-remote --instance` already had, so a
  throwaway verification run no longer has to share `repositories.json` with
  Boss's production instance.
- **`tuic <dir>` no longer registers a temporary directory as a repository.**
  Registration is permanent, so a temp path became a sidebar row pointing at
  something the OS later deletes — fifteen such rows accumulated over two days
  with nothing recording where they came from. `tuic` now refuses a directory
  at or below the temp root of the shell it runs in (`TMPDIR`, `TEMP`/`TMP`) or
  below `/tmp`, and points at `tuic new <dir>` for a shell there instead. The
  check lives in the CLI rather than the app precisely so a custom `TMPDIR`
  counts as temporary. Existing rows are unaffected; remove them with the
  sidebar's **Remove Repository**.
- **A Rust unit test can no longer read or write the real config directory.**
  `config_dir()` silently fell back to the user's platform directory when a
  test omitted `set_config_dir_override`, which is how disposable fixtures
  reached the live `repositories.json`. It now falls back to a safe,
  process-scoped temp directory instead — deliberately not a panic, which
  reproducibly deadlocked or aborted parts of the full test suite (a
  non-reentrant guard mutex, and a poison-on-panic interaction) rather than
  isolating cleanly.
- **The crash screen no longer leaks its copy-feedback timer.** The two-second
  reset that returns the "Copy error" button to idle had no cleanup, so it
  fired against a disposed signal when the boundary tore the tree down.
- **Fresh terminal tabs now retain their session alias.** The backend could emit
  `term-alias-assigned` before the frontend associated the new PTY session with
  its tab, so the event was discarded: the tooltip fell back to `Terminal N`
  and the context menu had no alias to copy even though MCP already listed it.
  Early alias events are now retained until that session binding exists.
- **MCP confirmation calls now allow the full answer window.** The stdio bridge
  previously aborted `ui confirm` after its generic 10-second response timeout,
  even though the server correctly waits up to 300 seconds for an answer. The
  bridge now reserves the server window plus five seconds of transport margin.
- **An unanswered MCP confirmation now reports its own timeout instead of a
  bare 408.** The HTTP server's outer request timeout and the confirm
  handler's own answer-window timeout were both 300 seconds, so the outer
  layer could win the race, drop the handler's future before its cleanup ran,
  and hand the caller `408 Request Timeout` instead of the documented
  `{confirmed:false, reason:"no answer within 300s"}` body — dropping the
  future also meant the pending `confirm_responses` entry leaked and no
  `McpConfirmResolved` event told any client to dismiss the dialog, so it
  stayed on screen after the caller had already moved on. The server's outer
  timeout is now 301 seconds, a deliberate 1-second margin over the confirm
  window (itself 4 seconds under the local bridge's 305-second wait), so the
  handler's own timeout always resolves first.
- **Remote devices can log in with Basic Auth again.** Settings → Services saved
  the remote-access password on its own, so a password could be stored while the
  username stayed empty. An empty username makes the server report the
  credentials as not configured and answer every attempt with 401, while the
  field still showed the `admin` placeholder that reads like a default. Saving a
  password now writes the username too, falling back to `admin` when the field is
  blank, and clearing the username while a password is set no longer disables
  authentication silently.
- **Post-merge cleanup no longer freezes the window while it runs.** Each step of
  the dialog — switch branch, delete the local branch, archive or delete the
  worktree, close the branch's terminals — called a command that ran inline on
  the IPC thread, which on macOS is the main thread, so the whole WebView was
  locked for the length of the git work. All four now run on the blocking pool,
  matching what their HTTP equivalents already did. Closing terminals was the
  worst of them: it waited up to 200 ms per session, so the freeze grew with the
  number of tabs on the branch.

- **Codex usage is visible in the active agent badge.** Claude and Codex already
  shared one polled usage ticker, but the general ticker hid that slot on every
  agent tab while the badge absorbed it only for Claude. A Codex tab therefore
  hid a valid Codex result on both paths. The badge now absorbs a usage result
  whose provider label matches the active Claude or Codex tab, opens the
  matching dashboard on click, and keeps a stale result from the previous
  provider hidden during an asynchronous switch.

- **The usage ticker no longer assumes Claude.** The polled provider defaulted to
  Claude whenever no agent tab was active, which on startup is every install. A
  Codex-only user therefore saw a Claude reading — in practice `Claude · no
  token`, since there are no Claude credentials to read — until a Codex tab was
  focused, the exact wrong-vendor number the shared slot exists to prevent. The
  poll now stays silent until a Claude or Codex tab is seen, and remains sticky
  on that provider afterwards, so switching to a shell still keeps the number on
  screen.

- **A child result no longer waits for another child to close before waking its
  parent.** Mail sent while an orchestrator was working was buffered correctly,
  but an idle shell could still report `working` until its asynchronous
  background-process probe settled. That settlement emitted the parent's own
  lifecycle state without reevaluating its pending inbox wake, so the result
  stayed invisible until an unrelated child event re-entered the router. The
  same authoritative probe settlement now submits the coalesced, payload-free
  inbox notice; result content remains in the FIFO inbox.

- **Copy Path on a tab gave a relative path.** The three tab-bar context menus
  copied `filePath`, which the tab stores keep relative to the tab's filesystem
  root, so the clipboard held `src/foo.ts` — resolving against whatever
  directory the consumer happened to be in. The same bare value was handed to
  the file-context Smart Prompts. Every Copy Path now routes through one helper
  that joins the root the tab already carries and shortens `$HOME` to `~`. Two
  file-backed tab shapes that never offered the item at all — the HTML preview
  and a `file://` plugin panel — now do.

- **"Open in Browser" on a `file://` plugin panel did nothing.** It went through
  the URL allowlist, which exists because terminal output is untrusted and
  permits only http/https/mailto, so the menu item silently logged "Blocked URL
  with disallowed scheme". A path the app is already rendering is not untrusted
  input and now takes its own route to the OS default application.

- **A workspace id that names no row no longer strands the terminals under it.**
  `setActiveWorkspace` wrote any id it was given, and a dangling one failed
  later in two places that named neither the call nor the repo: the sidebar
  rendered no tab row for the active workspace, leaving terminals alive with
  nothing to click, and the next terminal added to that workspace spawned in
  `$HOME` instead of the repo. The store now refuses an unknown id and keeps the
  previous pointer, which at least names a row that exists.

- **A skewed value in `repositories.json` no longer takes the app down on
  start.** The load-time repair replaced only `null` and `undefined`, so a
  `worktreePath` holding an object — written by a WebView still running the
  module from before `get_worktree_paths` changed shape — round-tripped through
  every later load and reached `joinPath`, which called `.replace` on it. Path
  and branch fields are now type-checked: a corrupt one degrades to "no separate
  checkout" and the next refresh writes the real value back.

- **A dictation longer than 30 seconds no longer loses the tail of every
  whisper window.** The final whole-recording pass ran with the flags that suit
  a single streaming window (`single_segment`, `no_timestamps`). Above one 30 s
  window those flags make whisper advance a full window on every step regardless
  of how much the decoder actually reached, so an early end-of-text token or the
  per-window token limit dropped the rest of that window for good — a 128 s
  dictation came back shorter than the partials shown while speaking. The flags
  are now chosen by audio length: a recording that fits one window keeps them,
  and the hallucination suppression they were added for with it. The no-speech
  gate also filters per segment instead of discarding the whole transcript on
  its worst one, so an ordinary pause mid-dictation no longer empties it.

- **Content search no longer grows the app until it is measured in tens of
  gigabytes.** Every repo switched to since 1.7.5 built a BM25 index that nothing
  ever released, so a working day across a few dozen repos left the backend
  holding one index per repo — 40.7 GB across seven, one of them 1.9 GB on its
  own. The resident set is now bounded by `index_memory_budget_mb` (1 GB by
  default) and the least recently used indices are dropped when it is exceeded.
  Never the repo that just built, never one still building, and never the only
  one left even if it exceeds the budget on its own — dropping that would
  silently remove content search from a real repo.

- **Editing one file no longer re-reads the whole repo.** A change to indexable
  content rebuilt the entire corpus, so a one-line edit in a 14,000-file repo
  paid for 14,000 file reads and a complete re-embedding. Only the files whose
  mtime or size moved are now re-read and re-embedded, and deleted files release
  their slot for reuse. A change set past a quarter of the corpus still rebuilds
  outright: the average document length BM25 normalises against is refitted only
  by a full build.

- **An AI agent run now survives a reload taken in the middle of it.** The saved
  conversation held the prose and nothing else, so coming back mid-iteration lost
  the tool cards, the loop state and the iteration counter — the reply was there,
  the work that produced it was not. The run is now written on every step of the
  loop, not only when a turn ends, and comes back as it was. Conversations saved
  by an older build load unchanged and are upgraded in place on the first read.
  Tool output kept in that log is redacted and capped exactly like the tool
  results already stored on messages.

- **Opening a very large file no longer freezes the app.** A 19 MB, 720k-line
  JSON put every line in the DOM and blocked the main thread for 12 s: the
  document was installed by dispatching a whole-document replacement into a view
  whose host is hidden behind the loading placeholder, which defeats CodeMirror's
  viewport calculation. It is installed as initial state now. Above the
  large-file threshold the editor opens as plain text with no highlighting and
  no gutter markers — the guard used to read the *previous* file's size, so a
  23 MB JSON got full highlighting and 409k gutter markers.

- **A terminal no longer opens as a small black box after a page reload.** Every
  terminal mounts before layout runs, so the first measurement lands on a 0×0
  pane and was simply discarded, leaving the canvas at mount-time geometry until
  a window resize happened to re-measure. A resize also cleared the row map
  before the PTY had answered, painting one blank frame per geometry change; the
  old frame now stays until the new one arrives.

- **Terminal output could be reverted by a frame arriving out of order.** Grid
  frames carried no order and were published after their lock was released, so
  two could land reversed — and because a delta's damage is consumed when it is
  cut, painting the loser reverted rows for good. Frames are now stamped inside
  the producing critical section.

- **Scrolling worked in the desktop app and did nothing in a browser.** The
  pending scroll target was only ever created by a desktop-only command, so a
  session no desktop terminal had rendered stored the target nowhere and still
  answered `{"ok":true}` — and closing the desktop terminal disabled scrolling
  for an already attached browser.

- **A hung `gh` could stop the window from ever appearing.** Boot ran
  `gh auth token` synchronously, twice, with no timeout. It now reads only the
  environment token before the window and resolves the keyring/`gh` chain on a
  deferred thread with a deadline. The AI scheduler no longer spawns its tick
  loop at boot with zero jobs, and the knowledge flush skips its dispatch when
  nothing is dirty.

- **The relay toggle works without a restart.** The client was spawned at boot
  only when the setting was already on, so there was nothing to receive a later
  "turn on" — and nothing ever sent on the shutdown channel either. It is
  supervised now, in both directions, and the backoff ladder comes back down.

- **A chatty SSH tunnel no longer deadlocks at "Connected".** stderr was piped
  but only drained after the process exited, so a full pipe buffer blocked the
  child forever and nothing flowed. It is drained while the process runs.

- **Unknown API paths return 404 instead of the app's HTML.** The server also
  serves the frontend, so every missing route answered `index.html` with a 200:
  clients read success, failed to parse HTML as a result, and reported a
  malformed response — which is how nineteen missing routes stayed hidden.

- **Network calls that could hang forever are bounded.** The shared HTTP client
  was built with no timeout at all, so a dropped VPN left the GitHub poller
  waiting on a socket that never answers; `Stop` can also preempt an in-flight
  poll now. Every network git subcommand reachable over HTTP takes the same
  fetch timeout the direct callers use, and so do PR/conflict fetches and
  worktree setup scripts. Calls into the mdkb daemon take a deadline and no
  longer hold the client lock across the round trip, which used to wedge every
  other caller in the process with them.

- **A file named `UUID.md` no longer marks a repository as conflicted.** The
  conflict check searched the whole porcelain output for `UU`, `AA` or `DD`
  without knowing which columns it was looking at; it reads the status field
  now.

- **A stale `index.lock` is adjudicated by owner instead of by age.** In a
  linked worktree `.git` is a file, so the sweep's `metadata` call failed and it
  returned early — silently, in exactly the place TUIC does most of its work.
  The pointer is followed now, `lsof` is asked who holds the lock before it is
  reclaimed, and the four possible answers (held, unowned, probe unavailable,
  probe timed out) are distinguished rather than collapsed into one `None`. A
  leftover lock in a submodule gitdir — 0 bytes, 11 days old here — is now
  named in the error instead of failing opaquely.

- **Asking the log viewer for errors no longer comes back empty.** It sliced the
  ring buffer to the newest N entries and filtered *after*, so a level filter
  found nothing whenever the newest N were a different level — which is the
  normal case, and the one query you reach for when something breaks.

- **Five UI preferences were silently discarded on the way to disk.** The
  frontend sent the outline, references, AI triage, AI chat and file-browser
  view settings; the backing struct declared none of them, so serde dropped all
  five without an error and they could never be given back. Three of those
  panels were also missing from the persistence functions entirely.

- **A plugin re-registering no longer leaks its watchers.** A WebView reload
  re-runs registration for every loaded plugin, and only the new capability set
  was inserted — so each reload stranded another copy of every plugin's
  filesystem watchers, accumulating for the life of the app. The plans plugin
  also rescans when the active repository changes, instead of keeping the
  previous repo's plans open forever.

- **Cross-kind tab drag reorder works.** The two store functions that maintain
  the cross-kind order list had no production caller, so the list was always
  empty and the reorder silently degenerated to insertion order.

- **A panic in a terminal's reader thread no longer leaves its timers running**,
  and the resize grace no longer re-arms on every chunk from a full-screen
  agent. Agent polling timers stop restarting on every tab open or close, which
  had prevented the 30 s fallback poll from ever firing during tab churn —
  precisely when session state changes most.

- **Dictation and audio no longer panic on an unusual device.** A 0-channel and
  a 0 Hz device each aborted the audio thread; two unbounded buffers are
  bounded; a poisoned limiter no longer stays poisoned. A single terminal cell
  can no longer absorb combining marks without limit (backported upstream
  Alacritty fix).

- **The same API error notifies every time, not once per session.** The dedup
  was re-armed on a parser event that nothing could ever construct, so the
  branch was unreachable.

- **A quiet AI conversation notices when the client disconnects.** The bridge
  waited on the next event to fail to send, so a close on an idle conversation
  went unseen.

- **MCP `initialize` echoes the protocol version the client offered** instead of
  answering a fixed one, and declares the `tools.listChanged` capability it
  already honours. OAuth authorization flows run concurrently — a single permit
  wrapped the whole browser round trip, so a second Authorize click blocked for
  the full five-minute timeout with no browser, no dialog and no error.

- **Cross-repo search stopped promising a retry nothing would honour.** Under
  the default indexing strategy an unvisited repo is queued for nothing, so
  "N still indexing, retry shortly" was false and retrying changed nothing.

- **The Codex dashboard refreshes.** It fetched once on mount with no interval
  and no cleanup, so the numbers froze until the tab was remounted, and a stale
  error never cleared.

- **The sidebar no longer contracts when the quick switcher is armed.** Holding
  the shortcut swaps the actions box for the hint, and the box it replaced was
  the only thing holding a branch row at its height.

- **A hidden terminal's frame acknowledgement has a wider margin** (300 ms
  against a 500 ms deadline), because that timer runs on the WebView main
  thread while the deadline is measured on the backend's clock.

- **Performance:** one `ps` fork per stats refresh instead of one per session,
  and no session lock held across the walk; one porcelain read per save instead
  of two; AI-watcher classification moved off the tokio worker with the config
  persisted outside the lock; the agent-injection Enter gap handed to a thread
  instead of blocking a tokio worker at three remaining call sites; link
  detection rect-tested before it runs and URL rows batched.

### Added

- **A workspace created for an agent comes with instructions.** The MCP
  response says it is a linked worktree, that refs and objects are shared with
  the parent, which ignored build directories arrived warm and how large they
  are (so no install or full build is run to "set up"), and that tracked parent
  changes are not carried over.

- **One terminal, three addresses.** Every `session` action that takes a
  `session_id`, and `agent action=send`'s `to`, now accept the PTY id, the
  `tuic_session` the tab persists, or the repo-derived alias (`tu-1`)
  interchangeably. An alias also survives a restart: the frontend replays it at
  create time and the backend reserves it, advancing the per-prefix counter past
  it so the next auto-assigned alias cannot collide. A terminal tab's context
  menu shows its alias and copies it on click. An address that resolves to
  nothing is passed through unchanged, so a session whose process has exited can
  still be read.

- **`tuic-remote` can run isolated named application instances.**
  `--instance <id>` selects a separate platform configuration tree and OS
  keyring vault before password setup or daemon startup. Named instances never
  migrate or fall back to default state, and a vault failure stops a release
  daemon before it binds. Omitting the option preserves all existing paths,
  credentials, and migrations.

- **A repo dropped by the memory budget comes back without re-indexing it.** The
  index is written to `<data_dir>/content-index/` on the way out and reloaded on
  the way back in, so returning to a repo costs a stat walk instead of a full
  walk, read and re-embedding. A snapshot is always validated against the working
  tree before use and brought up to date through the same path a live index uses,
  so it can never serve content that disagrees with disk; one that fails to parse
  is discarded and the repo is rebuilt. Requires a local patch to `bm25`, which
  keeps its document embeddings private with no way to read them back
  (`patches/bm25`, one read-only accessor).

- **Spreadsheets open as tables instead of binary noise.** The registry gains
  `xlsx-preview`, which reads `.xlsx`, `.xlsm`, `.xltx`, `.xltm`, `.xlsb`, `.xls`,
  `.ods` and `.fods` through SheetJS and shows one sortable table per worksheet —
  the counterpart of `csv-preview` for the formats the code editor could only
  render as a zip. Cells carry the value the spreadsheet displays, so dates keep
  the workbook's own format. Large sheets are capped at 2000 rows and 200 columns
  and the panel says what it hid.

- **TUICommander can now drive ego.** A supervised agent process per
  connection, durable sessions, turns you can cancel or pause, and the
  questions the agent asks back — all of it reachable from the desktop app and,
  identically, from a browser or the PWA. Same request fields, same responses,
  same errors; the only difference is that the desktop gets its turn frames on
  a Channel and everyone else on a WebSocket.
  The binary it launches is a setting, never an argument: no request can choose
  what your machine runs, and correcting the setting takes effect without a
  restart.

- **Settings has a search box.** 123 controls across 13 tabs and 2 sub-panels
  could only be found by opening each tab in turn. Selecting a result opens the
  owning tab and scrolls to the field, falling back to its heading when the
  field is not rendered. The index is committed rather than scanned from the
  DOM — only one tab is mounted at a time, and mounting the rest would fire CLI
  status, mdkb status, GitHub probes and audio enumeration on every keystroke —
  and a drift test re-derives it from the sources, so a setting added without
  indexing fails CI.

- **The language picker exists, and translation actually works.** `t()` ignored
  its key and always returned the inline English, with an empty `en.json`
  behind it: the whole i18n layer was inert. It now reads the active locale map
  and falls back to the inline string only when the map has none; `en.json` is
  populated from all 843 call sites (756 keys), byte-identical to the fallbacks
  it came from, so no rendered English string changed. Settings → General gains
  the picker, offering only locales that ship a catalogue.

- **The three terminal block-display settings are reachable.** Block
  timestamps, scrollbar marks and block folding existed in the store with no
  control, so the only way to change one was to edit `config.json` — which the
  backend then dropped on the next save. They are toggles in General →
  Terminal now, with scrollback reflow beside them, and all five are persisted.
  Block folding also honours its own setting: the check moved into
  `toggleBlockFold`, where the shortcut, the command palette and any future
  caller converge, instead of sitting in one of them.

- **A provider card says whether the endpoint answers, not just whether a key
  is set.** A local Ollama that is not running used to look exactly like one
  that is. The card carries a Reachable / Not detected indicator and renders
  the backend's own explanation verbatim; a probe that could not run at all
  reports unreachable with the error rather than showing nothing.

- **Codex usage dashboard.** The same shape of data the Claude one shows, from
  the rate-limit endpoint the Codex CLI itself polls and from the daily token
  history, both reading the OAuth token from `~/.codex/auth.json`. Identity
  fields — user id, email, account id, the whole profile object — are stripped
  in the backend before either response leaves it. The usage ticker follows the
  agent the active terminal is running instead of always showing Claude.

- **Diagnostics can now name what broke.** `GET /diagnostics/memory` reports
  entry counts for every map that grows with sessions, clients or repos,
  measured bytes for the four that hold payloads, and the real
  `phys_footprint_bytes` (resident *plus* compressed — `ps` read 0.52 GB while
  the process held 40 GB). A tripwire logs that report by itself at 4 GB,
  always on. The WebView also beats every 5 s, so a blocked main thread is
  logged after 30 s of silence; and a separate watcher notices the window
  landing on `about:srcdoc` after a standby memory sweep and navigates back to
  the app on its own, which no heartbeat can detect because the app is gone
  rather than blocked.

- **The terminal answers OSC 10/11/12 colour queries** from the resolved theme.
  An app that asks what it is painted with used to get silence, and a querier
  with no reply retries forever. Indexed `OSC 4;n;?` queries stay unanswered on
  purpose — that palette is not tracked, and no fence idiom waits on them.

- **goose gets a ready-screen adapter.** It launches as a long-lived
  interactive process, so OSC 133 never clears and the tab latched busy for the
  whole turn; its spinner glyphs and whimsical, reworded messages defeat both
  generic signals. Detection now keys on the hints at each end — `Ctrl+C to
  interrupt` while a turn can be interrupted, `Enter to send` when the composer
  accepts input — captured live on goose 1.49.0 and shipped as fixtures. The
  interrupt hint is tested first so a working screen is never downgraded, and
  Ready demands the composer footer rather than merely the absence of a
  spinner: a false Ready is what lets auto-standby SIGSTOP a live turn.

- **Both dictation speech gates are settings.** The RMS floor was hardcoded low
  enough for room noise to clear it, which is how Whisper ends up transcribing
  an empty room; the right value depends on the room and the microphone.
  `rms_threshold` joins a new `no_speech_threshold`, which uses Whisper's own
  per-segment no-speech probability and so rejects whatever the model invents,
  not only the wordings someone remembered to list. Configs written before they
  existed keep the defaults.

- **Resume finds the conversation again.** Two halves. A tab now binds to its
  agent through the agent's own pid→session registry (Claude's
  `sessions/<pid>.json`, grok's `active_sessions.json`) instead of "the newest
  unclaimed file in the folder" — measured on a live instance, 3 of 6 Claude
  tabs held no id and one held another tab's, so every tab resumed into the
  same conversation. And the launch command is rebuilt from the live process's
  argv and env, because a shell alias is expanded before `exec`: a run config
  reading `c2` is really `claude --dangerously-skip-permissions` under a
  different `CLAUDE_CONFIG_DIR`, and resuming with the config's version sent
  `--resume` to a binary reading the wrong directory.

- **Session state is pushed, not sampled.** `list_active_sessions` ran once a
  second for as long as any terminal existed. The backend now publishes
  `session-state-changed` once per real transition on both transports, with the
  same body the poll returned, so one frontend applier serves both. A push is
  not a queue, so an SSE reconnect or a `lagged` frame triggers the same
  catch-up read used at mount.

- **Every `/dictation/*` route, and seven more, now exist.** `dictation_routes`
  was written but never declared as a module, so it had never been compiled and
  all twelve handlers answered 404; `shell-family`, `agents/detect-all`,
  `agents/open-in-app`, `notification-sound`, `relay-status`, `check-update`
  and `worktrees/run-script` had no route either. Desktop worked over IPC while
  browser and PWA clients got 404 on all nineteen. The parity gate that should
  have caught this could not fail — its probe read a 405 from the SPA catch-all
  as "route present" — and is now split across both languages: Vitest snapshots
  every `COMMAND_TABLE` path and a Rust test probes each one against the real
  router.

- **The reason a smart prompt cannot run is clickable.** It reached the user as
  a `title=` attribute, which can never be. It is a button wired to the
  Providers settings tab now, with the click prevented from running the prompt
  it just said was unrunnable.

- **A corrupt config file is kept, and the backups are bounded.** An
  unparseable `config.json` was replaced with defaults, and because defaults
  carry empty tokens the document was written back on the same startup — so the
  broken-but-recoverable file was gone on the first restart, deterministically.
  It is now preserved as `<name>.corrupt-<uuid>` like every other config file,
  and the five newest backups per file stem are kept (a count, not an age: a
  user who comes back a month later would find an age rule had deleted the very
  file they came for).

- **An AI stream reaches a detached panel.** Detaching unmounts the docked copy,
  so a stream the main window runs for that terminal — a watcher rule, an
  automation goal, a terminal context action — rendered nowhere at all. The
  detached window now receives a projection of it, re-checking the chat id at
  the receiving end so another terminal's stream stays on another screen.

- **Windows works, and the suite now runs there to keep it that way.** The
  build shipped a Windows installer that nothing had executed: the first real
  run on the platform reported defects no unix machine could see, and they were
  production code, not test plumbing. `~` reached the OS literally in repository
  paths, working directories and agent commands, because tilde expansion read
  `$HOME`, which Windows does not set. Plugin ZIP installs extracted nothing and
  reported success over an empty directory — the archive spells its entry names
  with `/` and the path they were stripped from is spelled with `\`. Reading
  another process's environment always answered "not set", one oversized read
  running off the end of the mapping and failing the whole call, so nothing that
  depended on an agent's own environment worked. The allowlist of variables a
  child inherits was written by analogy with POSIX and omitted `PATHEXT` — half
  of what `PATH` means on Windows, where `claude` and `npm` are `.cmd` shims —
  along with `APPDATA`, `LOCALAPPDATA` and `PROGRAMDATA`, where such a tool keeps
  its configuration. The progress store refused to open while a `-shm` file was
  locked, which there is every moment a second connection is live. Terminating a
  process left its grandchildren running. CI now builds and tests on Windows on
  every pull request, so the next one of these is caught before it ships rather
  than after.

### Changed

- **The MCP session and peer payloads stopped restating themselves.**
  `session action=list` drops `child_pid` and `foreground_pgid` — no action
  accepts a raw pid — and adds `tuic_session`; `background_work` and `standby`
  appear only when true. `list_peers` drops the per-entry `registered_at` and
  `mail_wake` and the top-level `count`, and adds `alias` and `session_id` for a
  peer that owns a live terminal. `agent action=send` drops `ok`, `accepted`,
  `buffered_in_inbox` and `recipient_has_terminal`, leaving `delivered` and
  `delivery_path` as the whole verdict; `spawn` drops `peer_registered`,
  `communication_ready` and `send_to`, which restated `parent_session_id`.

- **`is_caller` marks the tab the caller runs in, not the one it is bound to.**
  It compares the caller's identity against the PTY that identity owns, so an
  orchestrator no longer risks closing itself.

### Security

- **TLS stack patched for RUSTSEC-2026-0285.** `rustls` moves to 0.23.45,
  which no longer accepts TLS 1.3 handshake messages across encryption level
  boundaries; `rustls-webpki` and `aws-lc-sys` follow. Remote access, the
  relay and SSH tunnel HTTPS calls all sit on this stack.

## [1.7.6] - 2026-09-02

### Added

- **The toast about a tab parked in the wrong repo can now register that repo
  for you.** It already named the directory nothing claimed; finding it in the
  sidebar and adding it by hand was still your job. The toast now carries a
  *Register* button that does it, and the parked tab moves home on its own.
  Registration still only ever happens from your click — nothing registers a
  repository behind your back, because doing so would move the repo you are
  looking at.

### Fixed

- **Two windows on the same repository list now agree.** The desktop app, a
  browser tab and the phone app all talk to one backend, but a save told the
  others nothing: each kept working from the list it last read, so a rename in
  one window could be silently undone by the next save from another. A save that
  changes something is now announced, and the other clients take in what changed
  without losing whatever you were in the middle of doing. The repo *you* are
  looking at never moves, and nothing with an open tab is taken from under you —
  neither a whole repository nor a single branch someone deleted elsewhere, and
  what is kept stays visible in the sidebar rather than surviving in name only.

- **Repository settings could stop saving entirely, and only a log line said so.**
  A repo under active work changes its diff counts every few seconds, and every
  window recomputes them for itself, so two windows legitimately held two
  different — equally correct — numbers at the same instant. The save protocol
  treated any such difference as a competing edit and rejected the whole batch:
  29 consecutive saves failed on one repo's line count, and unrelated changes,
  including registering a new repository, were wedged behind it. Counts, merge
  state and last-used-tab no longer take part in that check. Renames, ordering
  and grouping stay fully protected.
- **A tab parked in the wrong repo now says which repo is missing.** An agent
  started through MCP runs in its parent's directory, so its tab can belong to a
  repository TUICommander does not know about. The tab was filed under whichever
  repo happened to be on screen, with nothing to explain it. It now names the
  directory to register, in the log and in a toast — register it and the tab
  moves home on its own.

- A `cd` no longer moves a terminal tab to another repo. The tab stays in the repo
  it was opened in; only a tab still parked with no owner is settled by a `cd`.
  Re-homing an owned tab left the sidebar on one repo while the pane drew a
  terminal from another, with the tab itself missing from the strip.
- Closed the second half of the same regression: the OSC 7 worktree coordinator
  re-homed an owned tab across repos as well, and switched the sidebar to the
  other repo when that tab was active. It now only follows a cwd *inside* the
  owning repo, so a worktree switch still moves the tab and a cross-repo `cd`
  does not.
- **An agent that describes a menu no longer reports itself as waiting** — the
  footer row an interactive menu draws was treated as proof that a menu is open,
  wherever it appeared. An agent that read another session's screen and pasted it
  into its own answer therefore raised the attention mark on its own tab, marked
  confident, which nothing retracts: the tab claimed the agent was blocked on the
  user for the rest of the turn while it was working. The row is now recognised
  only where a menu actually draws it — at the left edge of the screen — because
  everything an agent streams is indented inside its own frame. Real menus are
  unaffected.
- **A toast now says which repo it came from, and clicking it takes the whole
  window there** — an agent's toast arrives while you are looking at another
  repo, and the name of the speaker was nowhere on it. It now carries the repo
  as a badge next to the title, read from the caller's own working directory and
  falling back to the repo owning the terminal that spoke. Clicking it used to
  change only the active terminal: the pane drew a tab from a repo the sidebar
  was not showing, and the tab strip had no entry for it. The click now moves
  the sidebar repo, the branch, the pane group and the focus together.
- **Per-repository settings survive a restart** — every per-repo override was
  dropped the moment it was saved. The store sent its own camelCase field names
  to a Rust struct spelled in snake_case whose every field carries a serde
  default, so an unrecognised key was not an error: it was discarded, and the
  field read back as "the user set nothing". Only `path` and `color` — the two
  names that spell the same in both conventions — ever reached disk. A base
  branch, a setup script or a worktree toggle set for one repository therefore
  worked for the rest of the session and was gone at the next launch. Names are
  translated at the boundary now, in both directions, so what you set is what
  loads. Existing files are read unchanged.
- **A worktree an agent creates no longer stops you to ask about it** — the
  "Switch to new worktree?" confirmation was a blocking modal with a ten-second
  auto-cancel, and it is raised by exactly the case that cannot answer it: the
  dialog only ever comes from a backend-initiated creation (the MCP
  `repo worktree_create` tool or the HTTP route), never from the in-app "+"
  button. An orchestrator creating a worktree every few minutes therefore took
  the screen every few minutes to ask a question nobody was there to answer. The
  offer is a toast now, with a **Switch** button, and it stays in the bell after
  the toast fades, so an unattended run leaves the offers waiting for you instead
  of throwing them away. Whether your current tab follows is decided when you
  click, not when the worktree appeared — a running agent still stays on its own
  branch and CWD while the worktree opens in its own terminal. Raised by
  [@mmullane001](https://github.com/mmullane001) in
  [#120](https://github.com/sstraus/tuicommander/pull/120), which proposed a
  setting to turn the dialog off.
- **A fresh clone no longer fails its own test suite on git 2.34** — `shootout_commit_log`
  compares the gix commit log against the git CLI byte for byte, and `%aI` spells a
  zero UTC offset as `+00:00` on git 2.34 (what Ubuntu 22.04 ships) and as `Z` on
  newer git. The reference side of the comparison therefore changed shape with the
  git binary on the machine, and the test failed on a clean checkout even though
  both backends agreed on every hash, parent, ref and subject. The comparison now
  reads the two spellings of UTC as the one instant they both mean. Nothing about
  the app changes: the commit log is served by gix, which already emits `Z` on
  every git version. Reported and diagnosed by
  [@smuchow1962](https://github.com/smuchow1962) in
  [#117](https://github.com/sstraus/tuicommander/pull/117).

## [1.7.5] - 2026-08-27

### Fixed

- **A confirmation an agent asks for reaches every client, not just the desktop** — `ui action=confirm` opened a native operating-system dialog on the machine running TUICommander. Anyone away from that machine could not answer it, and the agent blocked until someone walked back to the keyboard. The request is now broadcast to the desktop window, every browser tab and the mobile app at once, and carries a push notification so a closed phone app still surfaces it. The first answer wins and dismisses the dialog everywhere else. Escape, the overlay and Enter all answer *cancel*, because the question is asked before something destructive. Unanswered after five minutes it returns a refusal that says it timed out, so a silence can never be read as approval.
- **A shell sitting at a prompt inside `sudo su` no longer reads as working** — An interactive subshell is a single command that never ends, as far as the outer shell's integration is concerned: the inner shell has none of its own and never reports the command finished. A tab parked at an idle root prompt stayed marked working — observed for 33 minutes. A plain shell that has been quiet for three seconds now has its running processes checked, and if all of them are shells or `sudo`/`su`, the tab goes idle. Real work under the wrapper — a `sudo dd` copying a disk in silence — still counts as working, and the first byte of output marks the tab busy again.
- **A link followed by a comma opens** — `see http://192.168.0.165:5000, then reload` was detected as a URL with the comma attached, which is a legal URL character but not part of this address. The result failed to parse and the click did nothing at all. Trailing sentence punctuation is now dropped, while a closing parenthesis the address itself opened is kept, so a Wikipedia link survives intact. What is underlined is exactly what opens.
- **The status badge is no longer repeated as a task underneath itself** — A row reading "Working" above a second line reading "Working" spent a line of a small screen saying nothing twice. An agent's spinner word is dropped when it merely restates the badge next to it, on the mobile session list and the Activity dashboard alike. A real task, such as "Waiting for background terminal", is untouched, and the progress bar is unaffected.

- **Installing the MCP bridge no longer rewrites the rest of your editor settings** ([#115](https://github.com/sstraus/tuicommander/issues/115)) — Zed, VS Code and opencode all keep their configuration in JSON with comments and trailing commas, which the strict parser rejected. A rejected file used to be treated as an empty one, and a 400-line Zed `settings.json` came back holding nothing but our single entry. JSON configs are no longer serialized back at all: the document is parsed into a syntax tree, exactly one member is replaced, and everything outside it is returned byte-for-byte — comments, key order and indentation included. The edit is re-read and checked before it reaches disk, the original is kept once under `mcp-backups/` the first time we touch a file, and an edit that changes nothing skips the write so an editor watching the file is not made to reload. A file that no dialect can parse is still left exactly as it was. Zed, Amp and Gemini go further and are no longer written at launch at all: their MCP list lives in the same file as every other preference, so the first install waits for the button in Settings → Agents. Settings → Agents also gained **Remove all MCP integrations**, which lists every client holding a bridge entry and clears them — uninstalling TUICommander used to leave each one reporting a missing MCP server.
- **Browser session discovery accepts a legitimate no-match result** — The HTTP transport treated a decoded JSON `null` exactly like a response with no body, so `/agent/discover-session` logged an error on every browser/PWA poll whenever there was no session to discover. A literal `null` now stays aligned with the desktop command's `Option` result, while a zero-length body, malformed JSON, and non-success status still fail with the command identified.
- **Browser and PWA AI chats keep their history** — Conversation autosave, close-time persistence, reload initialization, history listing and opening, and deletion now use the shared IPC/HTTP transport instead of returning early outside Tauri. Browser clients therefore use the existing conversation routes with the same payloads as desktop, while stale asynchronous loads still cannot overwrite a newer local turn.
- **Repository changes no longer disappear silently across windows** — Repository persistence now sends ID-keyed `mutationVersion: 1` deltas that are applied to the latest document under the cross-process lock. Non-overlapping changes compose, same-record conflicts are deterministic and visible, and queued local mutations are not dropped while another save is in flight.
- **Content-search highlights use JavaScript's UTF-16 coordinates** — Rust converts matcher byte ranges to zero-based, end-exclusive UTF-16 code-unit offsets before returning them, so highlights after accented characters, em dashes, and emoji select the intended text while ASCII ranges remain unchanged.
- **Smart Prompt Auto-execute now controls drawer execution** — User-created prompts honor the setting when they are run from the drawer, inserting editable text without submitting when disabled and using the shared agent-aware `sendCommand()` path for exactly-once submission when enabled.
- **The browser Command Palette is usable from its toolbar button** — Browser mode now mounts a focused palette with a fail-closed action allowlist, HTTP filename/content search, empty-state handling, and search IDs that reject batches belonging to another window; desktop actions and shortcuts remain unchanged.
- **The terminal stops inventing an empty command block at every prompt** — Two places listened for the same shell-integration marker on desktop, and the code they both call is not repeatable: the second delivery closed the block the first had just opened and filed it as a finished command with no command line, no output and no exit code. Roughly half of the block list that drives block folding and the scrollbar marks was these phantoms, and the 500-block limit therefore discarded real history twice as fast. Only the renderer listens now, and it subscribes before it waits for its fonts rather than after — that wait was the window the first prompt of a session fell into, and it used to be the removed listener that caught it.
- **One panel's search results no longer land in another panel's list** — Content search results are broadcast to whoever is listening, with nothing to say which search they belong to. Streaming results into the file browser while the command palette started its own search appended the palette's matches to the file browser's list and stopped its spinner on the palette's last batch. Every search now carries an identifier that its results and its errors echo, and a panel accepts only its own. A search that is cut short because another panel started one still reports that it finished, under its own identifier, keeping whatever it found before it was superseded — otherwise the panel that asked first would wait for an answer that only the other panel can hear.
- **Opening a conversation from the history no longer blanks it** — The chat panel asked the backend for the conversation's live state whenever the conversation changed, and that state has no producer: the answer was always empty, and applying it erased the messages that had just been read from disk. The request is gone until something produces that state.
- **An improvement scan publishes its proposals once** — The scan wrote the same five proposals into the same place twice per run, once from the command's answer and once from the notification that carries it to every window. Only the notification writes now.
- **Web clients stop losing events while a panel opens** — A browser or PWA client subscribes to the event types it is listening for, and mounting a panel adds a type. Widening that subscription meant reconnecting, and the replacement stream subscribed only after the old one was gone: anything published in between reached neither, and nothing replays it, so a repository could stay stale until the next unrelated change. The subscription is now widened on the open connection. A detached panel that fails to receive its first snapshot is retried instead of being recorded as up to date, and the interface preferences, pane layout and activity list are written out on exit rather than dying with the half-second timer that was still waiting to save them.
- **Panels refresh for a tracked `build/` or `out/` directory** — Those two names were treated as build output wherever they appeared in a path, so an edit to a directory of tracked release scripts, or to a package named `build/` in a monorepo, was classified as noise and refreshed nothing. Only `.gitignore` decides now, and it is consulted for parent directories too, so generated ones stay excluded. Two ignore rules were also read wrongly: a root `.gitignore` now correctly overrides `.git/info/exclude` for the same path, as git does, and editing `.git/info/exclude` takes effect without a restart instead of leaving the rules cached for the lifetime of the watcher.
- **Plugin output watchers see every line, and no invented ones** — The raw output event carried to the interface was rate-limited by discarding the chunks that arrived inside its window. That is corruption rather than loss: the interface reassembled lines across chunks, so a discarded chunk joined the tail of one chunk to the head of a later one and reported a line that was never on the wire. Rare-line detectors both missed real lines and matched fabricated ones. Line assembly and matching now happen once, in Rust, on the PTY reader thread — the interface no longer reassembles anything. Rust ships the matched lines in ordered batches of at most one per 100 ms, and drops none of them: a batch that fills up is sent early, the frame ticker drains the tail when the terminal goes quiet, and session teardown drains what is left. Each frontend holds its own watcher set, so a desktop window and a browser tab no longer overwrite each other, and browser and PWA clients receive watcher matches for the first time. A pattern the Rust engine cannot express — lookaround, backreferences — keeps matching in the interface as before, and Rust then ships every line for as long as one is registered. A line longer than 256 KB with no newline in sight is dropped whole rather than truncated, because the retained tail of such a line is itself a line that was never on the wire, and a command that exits without a final newline still delivers its last line. Each interface re-sends its set every 30 seconds: nothing tells the backend that a browser tab was closed, so that heartbeat is what distinguishes a live set from an abandoned one and what restores a set that was dropped to make room. Delivery of the batches is live, like every other event of this kind — output produced before a terminal is on screen reaches no watcher.
- **A talkative MCP server no longer grows TUICommander's memory** — The thread that reads an upstream MCP server's output accepted every line it wrote into a queue with no limit, and nothing reads that queue between calls. A server that reports progress or logs while idle could therefore hold as much memory as it cared to produce. A server that never ended a line grew a single string until the process died. The queue is bounded now — by line count, by total size, and by the length of any one line — and past the bound it discards the oldest message rather than pausing the server, which is what a message nobody is waiting for is worth. A call also waits on one deadline for the whole exchange instead of giving up after a fixed number of intervening messages, so a server that talks while it works no longer loses an answer it gave correctly, and no amount of chatter can keep a call from ending. That deadline now starts before the request is sent rather than after, and covers the sending: a server that stops reading its input used to leave a large request stuck half-written, with no timeout yet in existence and the server unusable until TUICommander was restarted.
- **Resuming an older session no longer erases what it knew** — Only the 40 most recently used sessions are loaded at startup, so a session resumed from further back had no record in memory. The first command run in it started an empty record, and the next save wrote that empty record over the file — the session's own history disappeared as soon as it was used again. The file is now read before anything is recorded for it.
- **Reconnecting an MCP event stream no longer closes the replacement** — A client that reconnects can be given a new stream while the one it replaces is still shutting down, and the old stream's cleanup released the session it no longer owned — including the channel the new stream had just subscribed to. The new stream saw its channel close and every later notification was lost. Cleanup now releases only the stream that still owns the session, and holds it for as long as the release takes — including when the session has already been closed from the other side, which was a second way the old stream could take the new one's channel with it.
- **Git panels no longer show the state from before your last action** — Working-tree reads that arrive together share one computation, and that sharing was keyed by the repository alone. A refresh triggered by staging, unstaging, discarding, reverting a hunk, committing or applying a stash could therefore be answered by a read that had started before the change, leaving the panel on the old state until something unrelated moved. The shared read is now keyed by the repository together with a counter each of those commands bumps, so a read that starts after a change can never be served by one that started before it.
- **Build Cleaner reports the space it actually reclaimed** — Trimming an artifact subtracted the size the last scan had estimated, so a build between the scan and the trim published a total no measurement supported. The backend now measures each target as it deletes and returns the byte count, and only a removal that succeeded is counted. Refreshing while a scan is running also waits for a scan that started after the click instead of receiving the result of the one already in flight.
- **Two saves of a session's history no longer undo each other** — The periodic save and the save on exit can run at the same time, and each read the session into a copy before writing it. The one holding the older copy could write last, leaving the older history on disk with nothing marked for retry — the newer commands were simply gone. The write now happens while the session is held, so no command can be recorded into a state that is already being written. Two further ways the same history could be lost are closed: the startup load no longer replaces a session that has been used since the application started, and a history file that exists but cannot be read is moved aside instead of being overwritten by the fresh record that replaces it.
- **Code search notices a file restored from an archive** — Whether the search index still matches the repository was decided by modification times alone, and the tools that restore files — `cp -p`, `rsync -a`, `tar -x`, unpacking a build cache — deliberately preserve them. Search kept returning the old text for as long as nothing else touched the file. The check now compares the size as well, which the same walk already reads.
- **A dictation recording that hits the length limit says so** — Audio kept for the final transcription is capped at five minutes, and passing that cap dropped the beginning of the recording with nothing but a log line to show for it: the transcription came back missing its start and looked complete. The dropped amount is now reported all the way to the interface, which tells you how many seconds were not transcribed instead of returning to `Ready`.
- **The dictation panel notices a model changed by another window** — Which model is selected, and whether it is on disk, is cached to keep the microphone meter off the configuration file while recording. Only this process invalidated that cache, so a model selected, downloaded or deleted by a second window was never noticed. The cache now expires after a second, which keeps the meter cheap and bounds how long another window's change can go unseen.
- **Terminal copy removes Claude's visual quote gutter** — Copying a multi-line Claude message no longer pastes the repeated non-breaking-space plus `▎` margin into Slack or other editors. The selection backend strips only coherent gutter runs, preserving isolated block characters, indentation, bullets, numbering, emoji shortcodes, and non-breaking spaces inside the content.
- **Pull-request state badges retain the PR number** — Sidebar badges now show the PR identity together with states such as Draft, Conflicts, CI Failed, and Ready instead of replacing the number.
- **Expanded terminal context no longer repeats its title** — The bar now uses one `Context` heading and distinct `Intent`, `Assignment`, and `Prompt` sections. The existing agent intent is shown first, and MCP instructions ask the model to declare it at task start as well as on material phase changes.
- **Awaiting-input state is turn-scoped and transport-consistent** — Historical questions no longer re-arm after a later response or completion, bare Enter clears waits through desktop and HTTP input alike, and an open MCP UI confirmation no longer blocks other agents' requests.
- **Agent state transitions cannot be dropped by a busy event consumer** — Sticky per-session state now uses a lossless reducer lane instead of depending on the best-effort broadcast bus, preserves every response-required OSC notification in a PTY chunk, keeps an unobserved shell in `starting`, and recognizes a non-empty input box above arbitrarily tall custom HUDs.
- **Agent-state captures preserve causality** — New `.tcap` diagnostics retain PTY input/output boundaries, ordering, and timing, replay the chrome cutoff and input-line state, and are tracked against a Claude/Codex scenario matrix.
- **Agent-raised notifications identify their repository** — `ui action=toast` now derives the caller's repository from its MCP session and working directory, shows the repository name in the toast and bell entry, and scopes the retained bell item to that repository.
- **A plain directory added as a repository no longer floods the log** — The status bar and the Git panel Changes tab asked a non-git folder for its working-tree status on every repository revision, so each filesystem event logged `not a git repository`. Both now check the registered repository kind first, while worktrees keep querying through their parent repository.
- **A long AI answer no longer slows down as it grows** — The chat panel re-parsed, re-sanitized and re-inserted the whole answer every time a token batch arrived, twenty times a second, and replaced the rendered subtree each time. The work per batch was the whole answer so far, so the cost of a reply grew with the square of its length and the panel became progressively less responsive the longer the model wrote. The live text now renders on its own slower cadence, independent of the rate the tokens arrive at; the first token still appears at once, the final text is never held back, and starting a new reply clears the previous one immediately rather than after the next render. Messages already finished are unaffected — they were never re-rendered.
- **The terminal view on a phone updates the lines that changed** — The backend re-sends the whole screen on every frame, and the view wrapped each line in a fresh object before rendering it. Nothing matched what was already on screen, so every visible line was thrown away and rebuilt on every frame — thousands of elements a second for a screen where typically one line had moved. Lines that did not change now keep their elements, and their text, colours and searchable content are computed once instead of once per frame. Filtering the output no longer re-reads every line on every keystroke either.
- **Codex-orchestrated subagents no longer lose their task description** — Spawn integrations whose schema has a task prompt but no `pty_description` now derive compact display metadata from that prompt. Explicit descriptions and clears retain precedence, and agent-specific launch and prompt-delivery paths are unchanged.

### Added

- **Managed agents accept commands with one authoritative MCP response** — `session action=submit` claims a confirmed-idle empty composer, serializes the raw-mode text/Enter sequence against concurrent writers, advances the existing composer FSM and turn epoch, and waits internally for child terminal movement. The structured receipt distinguishes rejection, complete write, uncertain write, acknowledgement and timeout without pretending local bookkeeping is application acceptance. It never queues or overwrites a partial draft, `/clear` uses the same path, and legacy `session action=input` remains raw with `ok:true` meaning PTY write only.
- **Legacy MCP negotiation advances to 2025-11-25** — the server, HTTP and stdio proxy clients, and `tuic-bridge` now agree on the legacy revision required by clients that negotiate down; HTTP requests carry `MCP-Protocol-Version`, while older clients that omit it remain accepted.
- **Patched HTTP/2 denial-of-service dependency** — `h2` is updated to 0.4.16 for RUSTSEC-2026-0258.
- **Orchestrated PTYs show task descriptions** — MCP `agent action=spawn` and `session action=input` accept an optional `pty_description`, shown above the terminal alongside the last submitted user prompt and updated through the existing event transports.

## [1.7.4] - 2026-08-12

### Added

- **Queued Compose commands can be inspected and removed individually** — The queue panel now lists pending commands in delivery order and lets you delete one without clearing unrelated follow-up work.

### Changed

- **The toolbar bell shows the newest notification first** — Recent messages no longer hide below older history when the bell is opened.

### Fixed

- **`tuic agent send` reaches a peer that has no terminal** — The command resolved its target as a PTY and wrote into it, so a registered peer without a terminal of its own — any external orchestrator — answered `Session not found`, while the MCP `agent action=send` tool delivered to the very same UUID. The CLI now speaks MCP to the one authoritative delivery path rather than carrying a second copy of the routing rules, so both surfaces agree on the route and land the payload exactly once. When the agent in the pane already holds the session identity, the CLI registers as `<session> (cli)` instead of taking the identity away and stranding that agent's inbox. Terminal injection keeps its agent-safe framing under the explicit `tuic agent type`, which sends the text and the Enter as separate writes — a raw-mode TUI treats a combined `text\r` as an unsent prefill, and top-level `tuic send` does not split it. Acceptance is also no longer announced as delivery: a message that nothing will surface now reports `Buffered for <peer> (inbox_only)` with the registry's warning, because reporting that as success is how a reply to a terminal-less agent vanished in silence.
- **Generic OSC 777 notifications no longer latch awaiting-input state from vague wording** — Notification confidence now follows the body: explicit permission and approval prompts remain confident, while idle-style messages stay retractable instead of leaving a permanent question badge.

## [1.7.3] - 2026-08-10

### Added

- **Compose can queue follow-up work without steering the current agent turn** — Shift+Ctrl+Enter or the queue button hands a command to the backend idle gate. User commands and peer messages share one FIFO and drain one item per idle window in acceptance order; the visible count and clear action select only Compose commands, so clearing them cannot delete a peer result waiting behind or between them.
- **Raw PTY capture provides reproducible agent-state fixtures** — The off-by-default `/diagnostics/capture` endpoint records a bounded raw stream per session. Captures, rather than the rendered and lossy session-output ring, are replayed through the same raw-marker, rendered-row, and hook-suppression composition as production.
- **Toasts stay readable in the toolbar bell** — A toast fades on its own, usually while the user looks at another window, so the message was gone before it was read. Every toast is now mirrored into a **Messages** section in the bell as it is raised, keeping its level and its action. The mirror is the single funnel every toast passes through, whatever raised it. **Keep toasts in the bell** (Settings > Notifications) turns it off for anyone who wants toasts to stay transient; it sits outside the audio block, because the bell is visual and must remain configurable on a machine with no audio output.
- **Agents can request a distinct Attention callback** — MCP toasts accept an `attention` sound that travels through both the desktop event and browser SSE paths. Native and browser playback share the triangular G4→G4→E5 double-knock motif while respecting notification volume and per-sound settings.

### Changed

- **“Manage in Settings” lands on the MCP block, and the tab that holds it says so** — The MCP popup opened the Services tab at the top, where the upstream server list sits below the fold, so the click looked like it had done nothing. The deep link now names the block it promises and the panel scrolls to it once the tab is in the document. The nav entry is called **Services & MCP**, because “Services” gave no reason to look there for MCP.

### Fixed

- **MCP bridge configuration is written only for agents that are really installed** — The writer creates every missing parent directory, so the launch-time pass used to create `~/.cursor/`, `~/.codeium/windsurf/`, `~/.gemini/` and friends for tools the user never had, after which other software reports Cursor or Windsurf as installed. Presence must now be proven — a foreign file in the target's config directory, or its CLI on `PATH` — before anything is written, and a directory that holds only our own entry does not count as proof. Settings > Agents still installs on demand: that is an explicit request, not a guess. The pass also covers grok, opencode, Droid, goose (YAML) and pi, and a config that does not parse — JSON with comments, broken TOML or YAML — is never overwritten with our single entry.
- **A tab no longer stays badged "question" after the question is answered** — Awaiting state cleared only on a typed non-empty line, a parsed busy tick, or a choice-prompt keypress. An approval dialog answered with a bare Enter produces none of the three, so the badge stayed on for the rest of the session with the prompt long gone. The silence timer now retracts a heuristic question once it leaves the screen. Confident prompts are untouched: agents that repaint while they wait are not answered just because the current frame does not show them.
- **`agent detect` reports every agent TUIC can launch, and only the installed ones** — The MCP action carried a hand-written list of four names, so it announced aider and goose with a null path while hiding gemini, grok, opencode, Amp, Cursor, Droid and pi even where they were present. It now enumerates the same set the rest of the backend uses and omits what it cannot find.
- **Repository rows line up whether or not the repository is a git repo** — The add-worktree button is absent for a plain shell entry, and its missing box pulled the trailing icons 27px out of column. The slot is now reserved.
- **Intent titles now replace spawn-assigned tab names without overwriting manual renames** — A named MCP agent was restored with `nameIsCustom=true`, conflating its launch label with an explicit user rename and silently blocking every valid `intent: text (Title)` update. Session metadata now preserves the name origin across reconnects; only user-renamed tabs remain protected.
- **Silent orchestration completions survive frontend reconnects and completion fires once** — Re-adopting a surviving HTTP/MCP session used to discard its remote origin, so the “silence remote completions” setting stopped applying after HMR or reload. Remote origin now round-trips through both session-list transports, and the BUSY→IDLE and process-exit paths share a per-cycle completion latch instead of chiming twice.
- **Generic OSC 777 notifications no longer leave a tab falsely awaiting input** — OSC 777 is a desktop-notification transport, so “needs your attention” can also announce completion and is not evidence that the composer expects a response. Only explicit permission, approval, or waiting-for-input wording becomes a confident question; the latter preserves plan and skill picker detection when native hooks emit no awaiting event.
- **Cleaning up a merged worktree asks before it destroys uncommitted work** — Archive and delete both end in `git worktree remove --force`, but the confirmation only appeared when the branch was zero commits ahead, so one commit was enough to have hours of unstaged work removed without a word. The commit count no longer enters the decision: every cleanup path — the merge action, the post-merge dialog, and the automatic sweep — passes one gate that stops whenever the worktree is not *known* to be clean, and a dirty check that fails to run counts as not clean rather than as a silent yes. The sweep, which runs unattended, keeps such a worktree and says so. Archiving also became what its name promises: the directory is moved to `__archived/` before git's entry is dropped, so the files it carries survive instead of being deleted first and moved never.

- **Re-authorizing an HTTP MCP upstream takes effect on the next attempt** — The client no longer keeps a second memoized bearer after the credential vault changes. Every attempt reads the current generation, and 401 recovery compares it with the exact rejected bearer under the refresh lock, retrying a newly exchanged valid credential without rotating it or calling the token endpoint.

## [1.7.2] - 2026-08-06

### Changed

- **`agent wait` and `inbox` keep your place, and one field names the delivery route** — `next_since` was emitted only when there were messages, so after a timeout the only recoverable value was `0` and the model reloaded its entire history. It is now always present, including on timeout and with zero messages, and `since` became optional: omit it and the server resumes from a per-identity read cursor, pass it to override, pass `0` for a deliberate replay. On the send side, `delivered_via_channel` is gone from every response. It reported one sub-route (the SSE push) but read as a delivery verdict — it is `false` precisely when a waiter or the terminal carried the message, which are the best outcomes. `delivery_path` already names all four routes and subsumes it. The field survives on the stored message as server-side forensics and in the `agent_msg` log line, where an operator, not a model, reads it.

- **An identity change can hand over its mailbox across MCP connections** — Retiring a superseded identity silently did nothing when the new one registered on a different MCP connection, stranding mail with no wake. `register` now accepts `replaces=<old_uuid>` and reports the outcome: `superseded_identity` and `mail_migrated`, or — when the old identity still owns a live PTY — `mail_stranded` with an `identity_warning`. That last case deliberately moves nothing: an identity with a terminal is a reachable peer, and taking its inbox would strand a working agent.

### Fixed

- **The find shortcut re-focuses a search bar that is already open** — Pressing it a second time did nothing: after clicking into the content the input has lost focus, but the bar is still visible, and an unchanged boolean notifies nothing, so the auto-focus effect never re-ran. Re-opening is now expressible as a state change, and every search surface behaves the same — terminal, code editor, diff, HTML preview and markdown.

- **Orchestrator inbox wake retries are bounded per unread-mail group** — A payload-free wake whose PTY write was uncertain could expire every five seconds and retry forever. Each unread group now gets its initial wake plus at most one retry after an uncertain result; a second uncertain result and all coalesced mail remain inbox-only until a successful inbox or wait observation clears the group.

- **Creating a worktree no longer moves a running agent into the wrong sidebar branch** — The “Move & Notify” action only changed the terminal's UI ownership while the agent process stayed in its original working directory, then interrupted the agent to explain that it had not moved. The prompt now opens the worktree in its own terminal and leaves the running agent attached to its real branch and working directory.

- **Managed Codex peers auto-register with their real terminal identity** — Codex intentionally filters the environment inherited by stdio MCP servers, while TUICommander's generated MCP entry configured only the bridge command. The Codex process had `TUIC_SESSION`, but `tuic-bridge` did not, so initialize carried no identity header and the peer's first `agent send` failed until it registered manually. New and existing Codex entries now whitelist `TUIC_SESSION` through `env_vars` while preserving other server settings.

- **The undici advisory is closed, and the test suite stops leaking a timer** — GHSA-4cwx-7wf7-3272 could not be resolved by pinning undici, because jsdom 29 reaches into the old line's internal modules; jsdom moves to 30 instead. Separately, the deep-link test left a pending debounce timer behind and now cancels it through the hook that already existed for that purpose.

## [1.7.1] - 2026-08-03

### Added

- **`tuic run` and a `tuic ls` you can read** — `tuic run pnpm dev` creates a session and starts the command in it. `tuic ls` prints short IDs instead of full UUIDs (targets already accept prefixes) and gained `--json` for scripts, `tuic capture -n 50` returns just the tail, and `tuic status` names the sessions that are waiting on you. Session targets now also match a name prefix, case-insensitively.

- **pi is a tracked agent** — A pi session was invisible to TUICommander: the process path resolves to the Node interpreter, so classification saw `node`, there was no screen adapter, and a finished turn left the tab busy forever — reproduced live at 71.6 seconds with the composer already back. pi is now classified from its own command line, its animated `Working…` footer and bare composer row drive busy and idle, and resume runs `pi --continue`. Session discovery over `~/.pi/agent/sessions/` stays unwired on purpose: `--continue` already resumes the newest session for the working directory, which is exactly what discovery would compute.

- **Completion chimes can be silenced for MCP-spawned sessions** — An agent orchestration turned every finished remote worker into a beep. Settings > Notifications gains an Orchestration toggle: remote-spawned sessions still land in Activity and bump the badge, they just do not chime. Locally created terminals are unaffected.

### Changed

- **An off-domain MCP authorization server is now a warning, not a block** — Discovery hard-failed with an issuer mix-up error whenever a gateway served metadata naming the real upstream identity provider — the normal topology for gateways, corporate proxies, and hosted tenants, with no way past it. Both issuer gates are advisory now: discovery warns and continues, and the consent dialog names the authorization server's origin so the decision belongs to the user. Discovery also tries the standard insertion form before the non-normative append form for path-bearing issuers (they describe different servers), falls through on any non-2xx candidate instead of aborting the chain, and reports the response body of a failed client registration instead of discarding it.

- **`agent send` and `register` now report whether a message will actually surface** — `send` answered `ok`/`accepted` for a message that only landed in the inbox, and `register` said nothing about whether the identity had a terminal behind it; together that is how an orchestrator's reply to an agent launched outside a TUIC PTY vanished with both sides believing it had been delivered. `send` returns `delivered`, a warning, and `recipient_has_terminal`; `register` returns `terminal` and, when false, states the consequence and the way out.

- **Debug builds carry line-tables-only debug info** — Full DWARF cost 82 GiB of `target/` for this crate alone. Panic backtraces keep file and line, and profilers keep their symbols; only debugger variable inspection is dropped. Measured over a full rebuild: 82.6 GiB down to 8.4 GiB.

### Fixed

- **Orchestrator inbox notices remain live without steering active work** — Orchestrator role is now declared explicitly by `agent register` and surfaced with the server-derived `mail_wake` capability instead of being inferred forever from one child spawn. Mail buffered while working is re-evaluated at authoritative idle/completed lifecycle, generic coalesced notices never hide payloads from `agent wait`, inbox/wait observation closes the read-versus-wake race, PTY I/O no longer holds delivery-gate locks, and ambiguous payload-free notices can expire and retry. Headerless orchestrators remain honestly inbox/wait-only because MCP/SSE has no trustworthy lifecycle or turn-starting wake surface.

- **Peer results no longer steer a working orchestrator or leak their payload into its composer** — A registered parent now keeps every peer message in the authoritative inbox. An active `agent wait` owns delivery; otherwise only an authoritatively idle/completed parent receives one coalesced, payload-free notice to call `agent action=inbox`. Working, awaiting, starting, missing, and unknown lifecycle states fail closed to inbox-only. Ordinary managed-agent delivery is unchanged.

- **`tuic .` opens the repository again** — a directory used to create a stray terminal session and leave the sidebar untouched; the repo-opening branch behind it could never run, and even when reached it refused any folder that was not already registered. A directory is now handed to the app as a repo: known ones activate silently, a new one is confirmed once and then added through the same path as the sidebar's "Add Repository". `tuic .` also stops registering the repo as `/path/.`.

- **`tuic send` no longer mangles text containing a key name** — key names were substituted anywhere in the argument, so `tuic send build "Enter the room"` pressed Return and typed "the room". Arguments are now matched as whole tokens, and the key set covers arrows, Home/End, PageUp/PageDown and the full `C-<letter>` range.

- **Workspace crates are actually tested** — `make check` and CI ran only the main package, silently skipping 267 tests: the entire `tuic-cli` crate plus our patched `alacritty_terminal` and `vte` forks. Both now run the whole workspace.

- **Fullscreen terminal output can be scrolled without corrupting shell history** — Alternate-screen applications now receive their own bounded, ephemeral scrollback, while persistent logs remain tied to the primary grid. Enter/exit transitions invalidate every renderer cache as one generation, and resizing a live fullscreen app no longer suppresses the first shell lines after it exits. Oversized refreshes remain byte-faithful, so repeated snapshots are expected rather than silently deduplicated. Editors and monitors that paint in place (`vim`, `htop`) produce no history at all; a pager like `less` paints its first page by printing lines, so it leaves roughly one screen of history behind — the same artifact iTerm2's equivalent option has.

- **Updating mdkb no longer leaves TUICommander connected to the old daemon.**
  The detached daemon now identifies its version through the existing ping, and
  TUICommander compares it with the installed binary before accepting a cached
  or global socket. A missing or mismatched version triggers `mdkb daemon
  restart`; the in-app installer performs that check before reporting success.

- **Quiet dictation no longer types "Grazie."** — Whisper answers near-silence with a subtitle credit learned from YouTube, in whatever language it guessed the silence was, and the filter behind the RMS gate only knew English phrases: an Italian bare `Grazie.` went straight into the terminal. All eleven offered languages are covered now, and the matching was split in two — a short thanks is dropped only when it is the entire transcript, so a dictated sentence that contains one is no longer destroyed, while channel boilerplate is dropped wherever it appears. The Amara credit is matched by its domain, which catches every translation of it at once.

- **Scrolled-off output is no longer deleted from the mobile log** — Agent chrome was trimmed from each batch of lines as it left the screen, anchored on any separator row. Tool output, markdown tables and Codex's own divider lines all carry box-drawing runs, so a false positive silently dropped the rest of the batch from history for good: the mobile log showed paragraphs starting on their second line, and user messages echoed on a prompt row vanished. Chrome is now marked rather than truncated — readers skip it, the lines stay — and only a genuinely empty prompt row anchors the cut, never a blockquote or a table rule.
- **Renaming a file to a different case now works** — `rename_path` canonicalized the destination, and on a case-insensitive filesystem (macOS APFS, Windows NTFS) `readme.md` resolves to the existing `README.md`, so the rename collapsed to `rename(X, X)` and did nothing at all — no error, no change. The destination now keeps the spelling that was asked for; only its parent directory is resolved, so the repo boundary check is unaffected.
- **A scrollbar column no longer pins a tab busy forever** — Grok paints a scrollbar down the right edge once its output outgrows the viewport, which trims every otherwise-blank row to a single block glyph. Spinner detection read that as Aider's animated bar, so the screen never classified as ready and the tab stayed busy for the rest of the process — reproduced live on a finished turn still reporting busy two minutes later. Aider's bar always carries more cells plus its status text, so it is untouched.
- **`agent detect` reports every supported agent** — The surface hardcoded four binaries, leaving Grok, Gemini, OpenCode, Amp, Cursor, and Droid invisible to an orchestrator even when installed, and the HTTP route rejected them as unknown. Both routes read one shared constant, and a test parses the frontend registry to fail the build if it gains an agent the backend does not report.
- **A finished Grok turn goes idle again** — Grok moved its composer inside a rounded box, and the ready-screen adapter only matched a bare prompt character, which no current build emits. Ready never fired and the tab stayed busy — observed live 132 seconds after the turn had visibly finished, while the fixtures still encoded the older layout and passed. The spinner check still runs first, so a mid-turn screen remains working.
- **A worktree is no longer torn down mid-rebase** — During a rebase a linked worktree sits on a detached HEAD and reports no branch, which two paths misread as dead: the sidebar row vanished and its terminals closed mid-conflict-resolution, and the orphan scan raised an archive prompt. A worktree with a rebase, merge, cherry-pick, revert, or bisect in progress is never an orphan, and the pre-rebase branch is recovered from Git's own record — applied to both the CLI parser and the default gix backend.
- **Reverse video swaps both default colors** — Foreground and background resolution each received a single default and could only fall back to the one it was handed, so a cell with default foreground *and* background under reverse video painted an invisible block. That is the Pi composer caret, which made mid-text editing guesswork. Both now read the theme defaults, so no call site can get the pairing wrong.
- **Cmd+C copies a selection made inside a panel iframe** — Text highlighted in a same-origin panel (plugin dashboards, HTML previews, srcdoc panels) lives in that frame's document, so the host copy path found nothing and the keystroke silently did nothing after the native Edit > Copy accelerator had swallowed it. The focused frame is now walked into first; cross-origin frames are left to the browser.
- **Duplicate MCP registrations no longer flood the log** — Two bridge processes claiming the same session make the loser retry every three seconds forever, and each attempt logged a warning — 8149 lines in one day from three pairs, burying everything else. The first rejection per pair is reported, then one summary every five minutes carrying the suppressed count. Distinct pairs never hide each other, and idle entries are evicted so the map cannot grow unbounded.
- **Notes are no longer silently destroyed by a failed load** — Config loading returned an empty default on any read or parse error and hydration swallowed the exception, so a failed load left the store empty and the first edit atomically overwrote `notes.json` with an empty array — permanent, silent loss. A missing file is now distinguished from a corrupt one (which is moved aside rather than replaced), the HTTP route answers 500 on failure exactly as IPC does, and every mutation path is a no-op until a load has succeeded.
- **A worktree removed by the backend disappears from the sidebar** — Backend-initiated removals had no path to the store, so a removed worktree stayed listed and could even acquire a fresh terminal in its now-missing directory. Every removal path emits a `worktree-removed` event, bridged to SSE; the frontend closes the branch terminals and drops the row idempotently, and never touches the main checkout.
- **One blocking MCP wait no longer stalls every other request** — The bridge dispatch loop awaited each proxied request inline, so a single `agent wait` or `session wait` — parked server-side for up to 300 seconds — held back everything queued behind it, and an unrelated call sent in the same parallel block inherited the whole wait before its own transport timeout even started counting. Requests are now spawned per line and complete out of order, with whole-line stdout locking so responses cannot interleave. `initialize` stays inline and shares a lock with reconnection, collapsing concurrent reconnects into one upstream initialize; in-flight requests get a five-second grace period on stdin close.
- **A peer resolves to the terminal behind it instead of assuming a shared id** — An orchestrator running in a TUIC tab could not be woken by its own subagents: every send came back inbox-only while still answering ok, so messages piled up unread. The peer identity was serving as both mailbox address and terminal handle with only equality between them, and that equality never held — PTYs are keyed by a freshly minted uuid while the caller-supplied session id is exported separately. Delivery now resolves an announced identity to whatever PTY currently backs it, per call, so a respawn is picked up without re-registering. Four of the five session-creating paths injected no identity at all; all five now bind one, reusing the caller's stable identity where it exists and otherwise making the PTY key the identity. Teardown retires peer registrations filed under an announced identity, closing a leak where a self-registered peer outlived its terminal for the whole process lifetime.
- **Wake ownership is settled from proven delivery, not from the session existing** — Managed-PTY delivery returned whether the session existed, which is not a receipt: a busy composer only parks the message, yet the call still reported success. Every call site marked the message terminal-dispatched, and the waiter filter hides terminal-owned messages — so a message that had never been typed was both unowned by the waiter and unsent by the terminal. Delivery now reports what it actually did (typed, queued, or unavailable), and a queued message deliberately stays pending.

## [1.7.0] - 2026-07-31

### Added

- **Spawned agents now return a durable task handle** — `agent action=spawn` gained `task_id` and `poll_interval_ms`. The session exit path drives the task to `completed` or `failed` whether or not anyone is listening, so an orchestrator that disconnects mid-run no longer loses the outcome. Terminal states are immutable: a cancel that races the agent's exit wins and is never overwritten. Tasks live for the TUICommander process only — a restart tears down every PTY, so a recovered `working` task would describe an agent that no longer exists. Every field a pre-task client already read keeps its name and type, so classic MCP clients are unaffected.
- **New `task` tool to supervise work past the five-minute wait ceiling** — `agent action=wait` and `session action=wait` clamp to 300 seconds, which left an orchestrator supervising a longer-running peer with nowhere to go. `task action=get` answers immediately at every stage with status, result, or `error_detail`, and `task action=cancel` marks the task cancelled without killing the agent — `session action=kill` still does that. The tool is registered natively, so it is reachable through both the merged tool list and the collapsed/lazy discovery path. Its status vocabulary is the MCP 2026-07-28 Tasks vocabulary (`working`, `input_required`, `completed`, `failed`, `cancelled`) from day one.

### Changed

- **MCP `tools/call` no longer requires a protocol session header** — Identity is resolved per call instead: `mcp-session-id` is refreshed when present, and simply absent otherwise, so a plain `curl` against `POST /mcp` works for every action that needs no identity. Stale-session auto-recovery for long-lived clients is unchanged, and nothing was left unguarded — `register`, `send`, `inbox`, and `wait` already refuse on their own, the loopback guards sit downstream untouched, and `GET /mcp` still answers `401` without a header. Two pieces of wrong guidance were corrected alongside: `send`/`inbox` pointed callers at a `messaging` tool that does not exist (it is `agent`), and the register guidance claimed `x-tuic-session` could substitute for a protocol session — that header binds a TUIC identity to an existing session, it does not replace one.
- **Frontend application architecture decomposed** — The Solid application shell was split into focused hooks, stores, and components across roughly one hundred files, replacing a monolithic entry point. Behaviour is unchanged; the boundaries are now explicit enough for the runtime dependency-cycle checker to police them.

### Fixed

- **Opening a Markdown file for editing no longer stacks the viewer and the editor** — Terminals, diffs, Markdown, and editors each marked their pane active from their own store, so "only one pane shows" was a cross-store invariant that no store owned and every call site had to remember to clear the other three. MarkdownTab's Edit button did not, and rendered both at once. Activation is now exclusive inside the stores, which also removes the deferred effects that could never enforce it — re-activating an already-active tab writes the same value, so nothing fired and whatever else was on screen stayed there.
- **A new Codex terminal no longer adopts another project's session** — Codex is the only agent whose rollouts are not partitioned by project, and it was also the only one without a per-file age bound, so a fresh terminal took the globally newest unclaimed session. Discovery now applies the same age limit as its Claude, Gemini, and Grok siblings, checked before any file is opened, and filters candidates on the working directory Codex records in the rollout's first `session_meta` record.
- **Codex no longer flips idle while a background terminal runs** — Activity detection keyed on the verb `Working`, but Codex swaps that verb per phase and shows `Waiting for background terminal (41s • esc to interrupt)` while a spawned terminal is live. The screen read as ready, the silence timer flipped the session idle, and no signal could re-enter busy — a green idle dot for minutes on a working session, with a standby risk mid-work. Detection now keys on the interrupt hint anchored to its closing parenthesis, so prose mentioning `esc`, plain output bullets, and the past-tense `• Waited for background terminal` transcript line still do not match.
- **GitHub errors served as HTML no longer lose their status code** — The response was parsed as JSON before the status was checked, so any non-JSON body collapsed into an opaque decoding error and took the status with it. Classification was lost too: a `401` or an exhausted rate limit delivered as an HTML page was treated as a generic failure, so the token-candidate fallback never advanced to the next token and the circuit breaker counted an exhausted limit as an ordinary error instead of backing off.
- **Idle Grok tabs no longer remain active** — Grok's long-lived process leaves the shell OSC 133 busy marker set, while its composer remains visible during responses. TUICommander now recognizes Grok's animated bottom status row as `Working` and the stable `❯` composer after that row disappears as `Ready`, so tabs return to idle without requiring optional native hooks.
- **Grok can now invoke MCP tools proxied through TUICommander** — Grok rejects qualified tool names containing more than one namespace delimiter, which caused it to silently discard every upstream tool while still showing TUIC's native surface. Sessions identifying as `grok-shell-*` now receive the existing `search_tools`, `get_tool_schema`, and `call_tool` surface automatically; the compatibility mode is isolated to that MCP session, survives TUIC restarts through bridge initialize replay, and does not alter the global `collapse_tools` setting or other clients.
- **Frontend runtime dependency cycles now fail the standard validation path** — `make check` runs the production graph checker and its end-to-end harness, which proves an acyclic fixture passes and a synthetic cycle fails. The historical full-codebase review is now explicitly superseded instead of presenting stale or accepted findings as a live backlog.
- **Application overlay composition no longer forwards entire hook objects through a 43-property boundary** — Overlay inputs are grouped into narrow panel, Git, prompt, confirmation, utility, cleanup, and quit contracts. Git dialogs, confirmations, and post-merge cleanup now own cohesive render groups while preserving dialog order and the existing lazy Settings/Help boundaries.
- **Read-only MCP session and agent queries no longer churn the blocking pool** — Dispatch now offloads only actions that can sleep, spawn, write PTYs, inspect processes, or touch disk. Common list, output, status, peer-list, inbox, stats, and metrics reads execute inline without cloning their JSON arguments; blocking action behavior and panic-to-tool-error handling remain unchanged.
- **Cancelled SSH tunnel starts no longer leave profiles permanently stuck in Starting** — Tunnel startup reservations now carry ownership tokens and clean themselves up on failure or future cancellation without deleting a newer reservation. A concurrent stop prevents late supervisor publication, and `Started` is audited only after the live handle is successfully installed.
- **PTY startup no longer retries permanent failures or sleeps on async workers** — The shared retry policy now applies only to classified transient PTY-allocation errors. Invalid commands, cwd, or permissions fail after one spawn attempt, while Tauri and HTTP entry points run the bounded allocation backoff on Tokio's blocking pool so unrelated async work remains schedulable.
- **Terminal capability queries no longer lose replies during concurrent input** — PTY writes now serialize on a writer mutex independent from session metadata. User, HTTP/WebSocket, agent-injection, device-status, and kitty replies share the same ordered path, so reader-side replies can wait safely instead of being dropped by `try_lock` contention and leaving terminal applications hung or degraded.
- **Conflict Assist no longer reports stale-base checks as verified clean rebases** — A clean result now requires a successful refresh of the PR base from origin. When origin is unavailable, an existing tracking ref or local branch may still be used, but the result is explicitly `clean_unverified` and carries base provenance plus a visible warning; a missing base fails instead of attempting an invalid rebase.
- **GitHub validation and conflict responses no longer disable unrelated API operations** — The REST circuit breaker now counts only transport failures and `5xx` availability errors. Ordinary `4xx` responses remain caller-owned, while primary, secondary, and abuse rate limits still enter dedicated backoff even when GitHub signals them only in a `403` body.
- **Failed configuration saves no longer rotate vault-backed credentials after restart** — The session, relay, and VAPID secrets are now snapshotted and rolled back when a later vault operation or `config.json` persistence fails. Runtime config changes only after the vault and file both commit, and rollback failures preserve the primary error instead of being hidden.
- **Deleted worktrees no longer survive as ghost branches in the sidebar under repository-event load** — Repository structure refresh is now single-flight per repository with one coalesced trailing rerun. The previous latest-generation cancellation could obsolete every Phase 1 reconciliation during a sustained event stream, leaving deleted worktrees in the persisted sidebar cache indefinitely.
- **Codex internal continuations no longer remain falsely completed** — When Codex starts another execution cycle without a new PTY submission, its persistent goal footer may still say `Goal achieved` while the interruptible `Working` row is actively repainting. Movement of that exact semantic row now reopens BUSY and clears stale completion suggestions; a static Working row retained on a genuinely completed screen remains inert.
- **Agent tabs no longer flip idle while Claude or Codex is visibly working** — Codex now recognizes the newer `»` composer instead of anchoring activity to a historical `›` prompt, and Claude's semantic active phase outranks the empty composer it keeps visible during long tools. A live Claude phase also supersedes a premature Stop/suggest emitted before a blocking Stop hook finishes, while completed timing summaries remain idle-safe.
- **Terminal text snapshots no longer duplicate rows after viewport growth** — `format=text` now reads one canonical grid range instead of concatenating a retained log with the current screen, whose rows can overlap after a resize moves history back into the viewport.
- **Slash-command detection no longer floods the application log under output load** — The per-PTY-chunk debug record was removed from the parser hot path; a captured session produced more than one thousand identical records in eighteen seconds and coincided with repeated frame-backpressure resets.
- **F13–F20 can now be assigned to shortcuts and the Global Hotkey on macOS** — macOS never forwards these keys to the app's WebView, so the shortcut recorder saw no keypress at all and they looked unbindable, even though every layer beneath already accepted them. They are now captured natively and fed into the same recorder, conflict check, and storage as any other key. Keys that macOS itself consumes first — `F14`/`F15` drive keyboard illumination on many Macs — still need remapping in System Settings.
- **Saving part of the configuration no longer switches remote access off** — Config writes over HTTP and MCP replaced the whole document, so any field the caller did not mention fell back to its default; remote access defaults to disabled, which silently killed it on disk while the already-bound server kept serving, and the divergence surfaced only at the next launch. Partial saves now merge onto the current configuration, and all three writers rebind the listener when remote-access settings actually change.
- **A slow MCP tool call no longer stalls every other one** — Session close/kill, agent injection, and configuration saves all wait on real work; run directly on the server's async runtime they blocked unrelated in-flight requests for as long as they took. They now run on the blocking pool, and a handler that panics is reported as a tool error instead of taking the request down with it.
- **Absolute-path file endpoints reject directory traversal** — Containment in a registered repository was checked path-component-wise, which accepts `…/repo/../../etc/passwd` as being inside the repository while the operating system resolves it elsewhere. All six external-path routes now refuse traversal syntax, NUL bytes, and relative paths before that check. Symlinks placed inside a repository keep working — they are deliberate.
- **Suggested actions survive the narrowest terminal panes** — When the pane is so narrow that the colon of `suggest:` lands alone on its own wrapped row, the parser required a space that this shape never has and dropped the whole suggestion. Captured live from Codex at 9 columns.
- **Suggested actions survive a narrow terminal pane** — Rejoining a `suggest:` keyword split by wrapping now skips the agent's own hanging indent on the continuation row. Codex soft-wraps its output with two leading spaces, so in a narrow pane it emits `suggest` / `  : [ … ]` and the keyword never rejoined — the whole suggest was dropped, verified live at 9 columns.
- **Streaming agent output no longer stalls behind an unterminated synchronized update** — TUICommander advertises synchronized output (DEC mode 2026) and now enforces its 150 ms deadline instead of only ending an update when the program sends the closing sequence. A repaint whose terminator is delayed or lost previously buffered invisibly for as long as the session lived, which made heavy Codex streaming appear to swallow text and then reveal it all at once, and let any file containing the opening sequence wedge a tab permanently. Session teardown now also drains a still-buffered update instead of discarding it.
- **A working Codex agent is no longer reported as completed for the rest of the session** — Status-line dedup is now scoped to the turn instead of the whole session. Codex names every turn `Working`, so the second and every later turn had its status line suppressed; that event is the only thing that clears the previous turn's suggested actions, which the session snapshot reads as a completion marker, leaving an actively working agent shown as completed and idle with no further activity recorded.
- **MCP OAuth confirmation no longer appears after cancellation** — Authorization consent now uses the in-app modal, preventing the hidden macOS sheet from surfacing only after the pending OAuth flow was cancelled and opening an already-invalid authorization URL.
- **`tuic` CLI session and agent commands now match the backend contract** — `tuic ls`, `tuic new`, `tuic kill`, `tuic agent ls`, and session name/prefix resolution read the fields the server actually returns (`session_id`, `cwd`, `display_name`, nested `state`) instead of the non-existent `id`/`name`/`repo_path`, so listings show real ids, names, repositories, and per-session status. Session creation sends `cwd`, and `tuic new -n` applies the name through the dedicated endpoint rather than silently discarding it. `tuic agent spawn` takes the initial prompt the backend requires, with the repository moved to an optional `--repo` flag.
- **Terminal tab progress bar is no longer hidden by the active-tab accent** — The progress indicator is layered above the active tab's accent bar.

## [1.6.3] - 2026-07-22

### Changed

- **Dependency stack modernized** — Updated the frontend, Tauri application, CLI, website, icon plugin, and relay service to their latest compatible releases, including Vite 8, Vitest 4, TypeScript 7, Tauri 2.11, tower-http 0.7, tokio-tungstenite 0.30, rodio 0.22, rusqlite 0.40, and current cryptography crates. The dev overlay keeps the native TypeScript 7 CLI while using Microsoft's TypeScript 6 compatibility API for watch mode. Unused direct dependencies and duplicate build/runtime packages were removed; Web Push VAPID signing now uses P-256 directly instead of the vulnerable transitive RSA backend, and Wayland code generation consumes the upstream quick-xml 0.41 security fix.

### Fixed

- **MCP server changes no longer falsely demand an AI-session restart** — TUIC now reports that connected clients were notified through `tools/list_changed`, accurately distinguishing automatic refresh in compatible clients from the possible reconnect required by clients that ignore the notification.
- **MCP native file tabs no longer open without a visible tab** — Focused `ui action=tab` requests using an absolute `tuic://open` or `tuic://edit` path now switch to the owning registered repository before activating the repo-scoped file tab, preventing ghost content under an unrelated active repository.
- **Reliable Nightly publishing across Linux and signed-tag setups** — The quick-xml security update now patches the published Wayland scanner source instead of mixing its unreleased generator ABI with stable Wayland crates, and the `tip` tag no longer opens an editor when Git is configured to sign tags.
- **Activity Dashboard now agrees with ready agent tabs** — A ready input composer is shown as idle even when Codex retains a long-lived background terminal such as a development server; backend lifecycle tracking remains unchanged.
- **Agent lifecycle no longer sticks or flickers at terminal UI boundaries** — A current-turn `suggest:` completion marker now prevents a stale Codex Working row from relatching BUSY, while confirmed background descendants still keep the task working. Claude can recover from a missed idle hook after real turn activity once its empty composer remains stable, and animated status rows no longer erase a visible choice prompt or its `awaiting_input` state.
- **MCP peer sends no longer create ghost or missing turns** — `notifications/claude/channel` is used only to add messages to an already working Claude Code turn. Idle or completed managed agents, including Claude and Codex, receive the PTY split-write payload plus Enter so a successful SSE broadcast cannot consume wake-up ownership without submitting a new turn. Channel/inbox delivery alone no longer clears completion or reports the recipient as working.
- **Blocking MCP waits now honor their advertised deadline** — Agent inbox and session lifecycle waits sleep on events instead of polling. Both default to 60 seconds, cap at 300 seconds, and the stdio/socket bridge derives its read deadline from the requested wait plus a five-second transport margin for direct and collapsed calls, eliminating the unrelated ten-second IPC cutoff.

## [1.6.2] - 2026-07-20

- Fixed local MCP socket stalls under multi-agent load caused by re-entering an `input_buffers` DashMap shard while its entry guard was still held; bridge health checks now use constant-size MCP `ping`, report endpoint availability accurately, safely reclaim stale peer bindings after reconnect, reject initialize auto-bind takeover while the prior bridge remains live, and reuse the bridge's existing session for its proxied initialize so its own SSE stream cannot block identity binding.
- **Lean MCP orchestration contract** — TUIC connection acknowledgment is now explicitly once per MCP connection/reconnect, upstream tool descriptions no longer duplicate TUIC context, and multi-agent guidance uses the real `agent`/`session` primitives. Blocking waits default to 60 seconds and support up to 300000 ms through the bridge. Headerless callers can register without a PTY or supplied UUID, session lists mark the caller's managed PTY, and lifecycle notifications are documented as state-only while task results travel through `agent action=send`.
- **Lower-token MCP orchestration responses** — Successful `agent wait` calls now inline every retained new message (up to the 100-message inbox capacity) with a per-recipient logical `next_since` cursor that disambiguates equal-millisecond bursts. `agent send` returns the managed recipient's shell/agent state, while generated external peers use inbox-only delivery when no waiter or SSE stream is available. Session/peer/spawn/status/output responses omit absent optional values instead of repeating `null`; wait timeouts and screenshot success no longer repeat obvious next-step hints. Native MCP tool values now use compact JSON text, while valid upstream `CallToolResult` objects pass through unchanged—including `isError` and structured content—on both direct and collapsed `call_tool` paths; malformed upstream values retain the compact text fallback.

### Fixed
- **Dev/release repository persistence collision** — Debug builds now use a one-time production-seeded `~/.tuicommander-dev/repositories.json`, preventing a concurrently running development frontend from overwriting the installed app's repository list without changing unrelated config paths.
- **Background agent commands no longer look complete at a ready prompt** — Session lifecycle waits for a process snapshot newer than the ready observation, keeps `agent_state=working`, and defers parent idle mail while meaningful descendants such as Cargo/test processes remain alive, without conflating that task state with terminal input readiness. Persistent integration helpers and Claude's standalone timed macOS `caffeinate` assertion do not pin agents working; command-wrapping `caffeinate` remains meaningful, and process scans stop when no probe or background work remains.
- **Claude spawn no longer reports an early false idle** — Claude's echoed `❯ task` transcript row is no longer mistaken for its empty input composer while the submitted turn is still running; drafts remain non-idle and movement continues to drive working state.
- **Build Cleaner no longer rescans every repository after each HMR reload** — Concurrent scans now share one backend walk, recent results are reused briefly by exact normalized repository set, and the dashboard's explicit Rescan action still forces fresh filesystem data.
- **MCP-spawned agent names remain stable in terminal tabs** — The assigned display name now travels with `session-created` across desktop and SSE transports and is restored as a custom name after reconnect, preventing repository-derived OSC or intent titles from overwriting it.
- **Agent completion is no longer reported as generic idle** — Session list/status and HTTP session state now expose task-level `agent_state` separately from PTY `shell_state`. The explicit `suggest:` end marker reports `completed`; silence without that marker remains `idle`, and spawned-agent lifecycle mail preserves the same distinction.
- **Autonomous agent delivery no longer corrupts active turns** — Idle-to-busy injection now uses an atomic composer claim, publishes the idle event before any queued wake-up can make the session busy again, and submits at most one queued message per agent turn. The MCP bridge reads bounded HTTP response bodies instead of waiting for socket EOF, preserves a valid identity across one-off transport errors, and repairs identity bindings on subsequent calls. The CLI now bounds IPC waits and submits agent messages as separate framed payload and Enter writes.
- **Codex MCP spawn and submit reliability** — `agent action=spawn` composes structured `model` with explicit `args`, derives Codex prompt and parser semantics from a direct Codex executable, includes the approval-bypass default for direct commands, and preserves authoritative run-config prompt placement. Wrapper run configs remain untouched by the bypass default and receive an additive validation warning. Structured parameters retain their existing composition, so a caller-supplied model is still appended to wrapper args. Interactive Codex/OpenCode sessions defer the task through the existing ready-screen injection path outside authored run-config argv. Combined MCP `session input` text + Enter now uses the required split-write gap for those prefill-only TUIs instead of leaving text unsubmitted in the composer. Claude's established input path and default spawn argv remain unchanged.
- **Spawned-agent communication bootstrap** — Every MCP-spawned child is now registered server-side with an inbox before prompt delivery. Registered parents and children receive explicit reciprocal addressing, while callers without a bound peer identity get a warning instead of a false bidirectional guarantee. If that caller registers later, existing children and any early lifecycle messages are linked to the new parent identity. Send acknowledgments now distinguish accepted inbox delivery from the optional SSE channel path. Deferred prompts emit a single parent error only if still undelivered after the internal timeout; successful delivery remains silent and requires no polling.
- **Peer messages no longer overwrite partially typed input** — Terminal wake-up injection now requires an idle recipient with an empty composer and no active question or approval. Messages remain queued while the user or agent has draft input; clearing or submitting the composer safely rechecks delivery, while `agent wait` continues to wake directly from the inbox.
- **Peer delivery no longer leaves ghost busy sessions or races blocking waits** — Idle composer claims now roll back only when PTY delivery provably wrote no bytes; partial or ambiguous writes remain conservatively busy and surface `delivery_uncertain` without an automatic duplicate retry. Per-message delivery leases atomically hand each wake-up to either `agent wait` or SSE/PTY delivery, including deadline and cancellation races. SSE delivery reserves the recipient's new task epoch before consumer visibility, restores it only on proven zero delivery, and serializes old-turn idle notification against new submissions.

### Added
- **Names for MCP-spawned agents** — `agent action=spawn` accepts an optional `name`, assigns it to both the peer identity and PTY session before prompt delivery, returns it in the spawn response, and preserves it when the child later auto-binds its MCP connection.
- **Independent PTY color environment** — New PTY sessions no longer inherit a Codex parent's `NO_COLOR`; terminal capability variables remain present, while explicit per-command color flags remain authoritative. MCP `session action=list` now exposes the assigned `display_name` alongside the independent repo-derived `alias`.

## [1.6.1] - 2026-07-15

### Added
- **Dictation live microphone meter** — The floating dictation preview now includes a compact centered spectrum meter whose bars widen outward from the center with your voice, so you can confirm that the selected microphone is receiving speech before the first partial transcription appears.

### Fixed
- **Status bar crash during Claude usage rotation** — The agent badge could crash the app (`undefined is not an object … claudeTicker().priority`) when the Claude usage ticker expired at the exact moment it was being rendered. The badge now uses a scoped match so a mid-render removal can no longer dereference a missing message.
- **Create Branch from a branch whose name contains a slash** — Basing a new branch on a local branch with a slash in its name (e.g. `POC-0001/merge-radar`) no longer fails with `'POC-0001' does not appear to be a git repository`. Such refs are correctly treated as local instead of being mistaken for a remote to fetch.
- **Agents no longer flip idle mid API-retry** — During an API connection-retry loop (e.g. `Unable to connect to API · Retrying · attempt N/M`) the agent's TUI freezes between attempts, which previously looked like completion. The session is now held busy across the retry gap until the agent recovers, the retries stop, or you re-engage.
- **CI auto-heal on partially completed workflows** — A failed job now reaches the branch agent immediately even when sibling jobs keep the overall GitHub Actions workflow running. Failed log lookup or PTY delivery no longer consumes one of the three heal attempts.
- **MCP upstream authentication editor** — Switching an HTTP upstream from Bearer to OAuth now clears the incompatible stored token, persists DCR mode when the client ID is blank, and reconnects with the selected method instead of silently remaining on Bearer.
- **Concurrent settings saves** — Frontend configuration writers now share one serialized load-modify-save queue, preventing overlapping General, Services, and plugin writes from losing each other's fields.
- **MCP bridge returning zero tools after a settings save** — Restarting the remote HTTP server no longer drops the runtime that owns the local Unix-socket MCP listener. Claude and other stdio clients remain connected to TUICommander instead of reporting `connected · no tools` while the web UI still appears healthy.
- **Settings no longer clobber the web-server toggle or global hotkey** — Changing any General setting used to overwrite `config.json` wholesale from a stale in-memory snapshot, silently resetting fields owned by other panels — most visibly turning the Remote Access web server **off** and wiping the **global hotkey** on the next restart. The settings store now uses a load-modify-save (fresh `load_config` → apply only its own fields → `save_config`), matching the Services tab, so `services.*`, `mcp_server_enabled`, and `global_hotkey` are always preserved from disk.

## [1.6.0] - 2026-07-11

### Added
- **DOCX Preview plugin + plugin binary reads** — The external plugin registry now includes `docx-preview`, which previews Word `.docx`/`.dotx` files as Mammoth.js HTML with raw-text fallback and conversion notes. PluginHost also gains `host.readFileBase64()` (IPC + HTTP parity) so file-preview plugins can handle binary formats without abusing UTF-8 reads.

### Fixed
- **Reliable agent busy/idle state** — Agent activity is now movement-based: "if the text above the input area moves, the agent is active". BUSY is latched and kept alive only by actual screen changes (text-equality diffed, so a frozen glyph — a completed-turn summary `✻ Sautéed…`, a `· run /mcp` hint, a HUD progress bar — can never pin a session BUSY), by user submission, and by lifecycle hooks; Claude/Gemini/Aider screen classifiers are prompt-based only, so an idle prompt is always reachable. Codex keeps its presence-based `Working (… esc to interrupt)` detection because its TUI legitimately freezes while a child process runs. Ctrl-C/Escape wait for confirmed interruption; observed hook busy state outranks silence; heuristic-only idle cannot trigger peer-message injection or auto-standby for adapter-backed agents. Manually launched Droid sessions now receive the agent idle threshold.
- **Context menus at viewport edges** — Large context menus and nested submenus now constrain themselves to the visible viewport and scroll when needed instead of opening partly off-screen.
- **AI Review on oversized PRs** — When GitHub refuses to render a PR diff because it exceeds the file cap, AI Review now falls back to a local-clone `git diff` instead of surfacing the raw 406 response.
- **File Browser "go up" in empty folders** — The `..` parent entry now shows even when a subdirectory is empty, so you are never stranded without a way back out.

## [1.5.2] - 2026-07-09

### Added
- **OSC 52 clipboard notice + toggle, and safer suggestion chips** — Clipboard writes emitted by terminal output (OSC 52) now surface a non-blocking "Clipboard updated by &lt;session&gt;" notice, and a new Settings → General → Terminal toggle ("Allow OSC 52 clipboard writes") lets you ignore them entirely. Suggestion chips (OSC 7770 `suggest=`, emittable by any displayed file/log) that contain shell metacharacters are now inserted without an automatic Enter, so a click can never silently execute a spoofed chained/redirected command — the user reviews it first. No provenance gating, so legitimate shell-integration clipboard/suggestion flows are unaffected.
- **"QR for Remote Mobile Connection" command** — A Command Palette entry that pops a large, black-on-white QR code in a dialog: scan it with your phone to open the mobile companion PWA, no typing. Reuses the Settings → Services connect flow (the session token is embedded server-side and never reaches the UI); shows a hint when Remote Access is disabled and a network picker when the machine has multiple IPs.
- **Multiple GitHub accounts (github.com + GitHub Enterprise)** — TUICommander is now account-centric instead of single-account. Add a second github.com account (device flow) or any number of **GitHub Enterprise Server** accounts (paste a Personal Access Token), and bind each workspace repo to the account that should monitor it. A repo with an ambiguous origin (multiple GitHub remotes or matching accounts) surfaces a **binding chooser** instead of silently picking `origin`. Every account gets its own isolated polling, rate-limit budget, viewer identity, and circuit breaker, so a rate limit or auth failure on one account never stalls the others. github.com-only setups are byte-for-byte unchanged — the OAuth device flow, token resolution, and single-batch polling behave exactly as before. Managed under Settings → GitHub → *Additional GitHub Accounts* and *Repository Bindings*.
- **Event-driven multi-agent orchestration (MCP)** — Agents connected over the MCP control surface can coordinate through TUICommander as the hub: discover peers, exchange messages, and receive push-style notifications driven by session events rather than polling.
- **Full browser / PWA / remote HTTP parity** — Git Panel writes, the GitHub panel and auth, AI Chat and the autonomous agent loop, the Claude Usage dashboard, plugin data/lifecycle, provider keyring, filesystem, and terminal reads now all have HTTP/SSE/WS equivalents. The mobile PWA and remote clients can now drive nearly the entire app, not just terminals.
- **Command Palette button in browser mode** — The toolbar exposes the Command Palette directly when running over HTTP/PWA, replacing the hidden desktop-only IDE launcher entry.
- **Merge with pending CI + squash default** — The PR Merge action is available while CI is still running and defaults to *Squash & merge*.
- **GitHub Ops improvement proposals** — The Ops Dashboard can scan a repo for refactor, testing, or performance proposals using the Headless AI slot, then create a GitHub issue only after explicit approval.

### Changed
- **Branch context menu reorganized by usage frequency** — The sidebar branch menu groups the actions you reach for most at the top.
- **Design-system audit — tokens, accessibility, GPU animations** — Canonical CSS token migration (no orphan/aliased variables left), keyboard accessibility (`role`/`tabindex`/Enter–Space) across collapsible headers and clickable rows, progress bars moved to GPU `transform: scaleX()`, and removal of side-stripe accents and mobile glassmorphism. Active/selected states now use a non-layout inset ring, so selecting an item no longer shifts its neighbors. Contributed by @paulovitin (#93).
- **Terminal grid: damage-tracked row diffing** — The PTY→grid `process()` path now diffs only the rows alacritty marks damaged instead of rebuilding and comparing the entire visible screen on every output chunk. Under spinner-heavy/flooding output (hundreds of chunks/sec) this drops the per-chunk cost from O(rows×cols) to O(changed rows). A byte-parity differential test pins the new path to the old full-diff output; the text-equality guard is preserved so no spurious or missed row events reach the parsers (slash-menu, choice-prompt, question detection). Backed by a new independent parse-damage view in our alacritty fork.
- **Per-session PTY event delivery (browser/remote WS)** — Each session's WebSocket handlers now receive parsed/exit/closed events over a dedicated per-session channel instead of every handler subscribing to the global event bus and filtering by id. Under many concurrent sessions with heavy output this removes O(sessions × subscribers) event cloning+matching on the hot path. Wire frames are unchanged; close notifications remain redundantly delivered over `/events` SSE.

### Fixed
- **Codex tab showed idle while working** — Codex freezes its terminal UI while a child process runs (a long `cargo`/`git`), producing no output, so the output-driven busy detection flipped the tab to idle after ~2.5s even though the agent was still working. The idle timer now treats the on-screen `• Working (… esc to interrupt)` status line as a liveness signal and keeps the tab busy until it disappears.
- **Context-menu items dropped + dangling separators** — A regression made every menu item that requested a trailing divider (the File Browser's New File, Paste, Delete, Add to .gitignore) render as *only* a divider, so those items vanished from the menu. Items now always render; a separator on the last item is suppressed so no context menu ends with a dangling divider (affected every menu — sidebar, terminal, file browser, git). The terminal menu also groups Copy and Paste together.
- **SSH tunnel remote forwards** — The tunnel editor now saves Remote forwards with `local_host`/`local_port`, matching the backend schema and preventing Remote forwards from being rejected as malformed Local-forward payloads.
- **Codex spawn reliability + intent parsing** — Hardened Codex CLI launching, intent parsing, and several related UI issues (#100–#104).
- **Browser / PWA mode fixes** — Killed a duplicate "PTY:" tab race, fixed worktree-create routing, wired scroll history and coalesced scroll-to-offset so browser-mode scrolling renders, forwarded the mouse wheel to the app whenever mouse reporting is on, and added `convertFileSrc` to the Tauri shim to stop a PWA crash.
- **iPad / touch input** — Fixed on-screen-keyboard focus (only the focused terminal lifts above the keyboard, anchored to the cursor row), touch-scroll direction, emoji-glyph rendering, and long-press to start drags so sidebar scroll still works.
- **Clipboard consistency** — All copy paths route through the shared `writeClipboard` helper.

## [1.5.1] - 2026-06-26

### Fixed
- **Git Log — expanded commit body overlap** — Expanding a commit in the Git Panel's Log tab no longer lets its (wrapped) body and changed-files list overlap the next row. The expanded section's real height is now measured rather than estimated from a line count that ignored wrapping.
- **Stuck "awaiting input" badge** — A confident interactive prompt (e.g. an agent's "Enter to select" menu) answered via a channel/MCP prompt box instead of a direct keystroke no longer leaves the tab pulsing amber. The badge now also clears once the terminal has been busy (producing output, no re-detection) for a short window; a statically-waiting prompt still keeps the badge.
- **Boot-time unhandled rejection** — Disposing a Tauri event listener that was already unregistered no longer surfaces an "undefined is not an object (listeners[eventId].handlerId)" rejection at startup; the async unlisten path is now swallowed.
- **Dictation docs** — Corrected the documented Windows whisper acceleration (CPU-only, not Vulkan/GPU).

## [1.5.0] - 2026-06-25

### Added
- **Native agent hooks for status (Claude, Gemini, Grok, Codex, OpenCode)** — Opt in per agent in Settings → Agents to drive busy/idle/waiting state from the agent's own hook system instead of output heuristics. Enabling installs small `OSC 7770`-emitting hooks into the agent's native settings format (Claude/Gemini/Codex JSON settings-hooks, Grok own-file, OpenCode plugin); disabling removes only TUIC's entries (sentinel-owned, so your own/wiz hooks are never touched). Instrumented agents report question/approval prompts precisely and suppress heuristic question-detection (the silence-idle crash backstop stays on). macOS/Linux only; Windows keeps heuristics. Default off — byte-for-byte current behavior.
- **Inline git blame in the code editor** — A dim italic "author · relative time · summary" annotation at the end of the editor's active line (GitLens-style), following the cursor over already-loaded blame data. Uncommitted lines show "You · Uncommitted changes". On by default; toggle via the `inline_blame_enabled` config field. External (non-repo) files show no annotation.
- **One-click delete-merged branch cleanup** — The Git Panel's Branches tab shows a broom button (with a count badge) that bulk-deletes local branches already merged into main, behind a confirm dialog listing the targets. Uses safe `git branch -d`, so a stale merged flag can never cause data loss.
- **"Active only" repo filter** — A filter icon in the toolbar (next to the sidebar toggle) hides repositories that have no open terminals. An accent banner at the top of the sidebar shows the shown/total count and offers "Show all". Session-only.
- **Context-menu shortcut keys** — While a context menu is open, pressing an item's shortcut chord (e.g. the branch menu's `M`/`R`/`d`) now fires that item's action directly instead of just closing the menu.
- **Two new built-in themes** — deep-black and minimal-kiwi.
- **Cmd/Ctrl+R reloads a web/preview tab** — When a web or HTML-preview tab is active, the Run shortcut reloads it instead of opening the Run Command dialog.
- **User-prompt markers on the terminal scrollbar** — Green ticks on the scrollbar mark the lines where you entered a command, so you can jump back to an earlier prompt at a glance.
- **Large files up to 250 MB in the code editor** — A dedicated read path raises the editor's file-size ceiling to 250 MB; files beyond the cap are refused up front with a notice instead of freezing the UI while loading.

### Changed
- **Branch merge feedback** — Merging a branch from the Branches tab now surfaces the result explicitly: a conflict error toast on failure, an "Already up to date" toast on a no-op, and a success toast with a one-click "Delete branch" action for the now-merged branch.
- **Readable theme contrast** — Raised muted / bright-black tones across catppuccin-mocha, darksun, delicate-one, nord, solarized-dark, tokyo-night and vscode-light so muted text clears a readable floor. The Appearance terminal preview now picks powerline segment text color by WCAG contrast against the fill instead of the literal ANSI token.
- **Unified PTY command injection** — The Notes "Send to Terminal" action (attached and detached), the mobile command/ideas widgets, agent auto-retry, the git terminal fallback, the CI auto-heal conflict prompt, and the terminal `writeln` API all route through the canonical `sendCommand` helper, so the Enter keypress registers correctly even for Ink-based agents in raw mode.
- **Responsive typing under heavy load (macOS)** — The PTY reader, frame ticker, and keystroke-write threads now run at `USER_INTERACTIVE` QoS, so typing and echo stay fluid while a saturating `cargo build`/compile runs in another pane. Complements the existing renice of PTY child processes.

### Security
- **Deep-link command gating** — `tuic://cmd/{tool}/{action}` deep links now default-deny: only read-only/notify actions run silently, while every other (destructive or unknown) action requires a confirmation dialog — closing previously un-gated paths such as `agent/send` and `session/pause`. A Rust-side backstop rejects `config/save` and `debug/invoke_js` regardless of the frontend. The `session pause`/`resume` MCP actions are now restricted to loopback clients like the other destructive session actions, and remote (TCP) clients get the standard 10 MB cap on the editor file-read routes instead of the 250 MB local cap.

### Fixed
- **No false completion sounds on sleep/wake** — Busy→idle transitions in the grace window after a system wake, and shell-integrated terminals that went busy without a command actually running (OSC 133), no longer fire spurious completion notifications.
- **Terminal stability** — Fixed a stale-read crash when a PTY auto-closes mid-frame, a document-listener leak when a terminal unmounts while subscribing, a layout-forcing read on every scrollbar drag, and a ghost-terminal resurrection from a late OSC 133 flush after removal.
- **Terminal interaction fixes** — Cmd+C no longer copies duplicated text and now unwraps soft-wrapped selections; a stale "awaiting input" badge is cleared when an agent exits back to the shell; and smooth-scroll self-heals canvas drift, snapping to the line when a scroll gesture ends.
- **Git Panel header alignment** — The Branches/Changes/Log/Stashes tab strip no longer reserves a horizontal scrollbar that misaligned the detach/close icons and left dead space; overflowing tabs scroll via vertical wheel/trackpad, and the close button is now an SVG icon matching the detach/reattach controls.
- **macOS dock-icon reopen** — Clicking the dock icon while the main window is minimized now restores it, even when a detached panel window is open (which previously suppressed the default un-minimize).
- **Resilient git operations** — Stale `.git/index.lock` files left by crashed processes are reclaimed on size+age thresholds; branch-switch failures show the full git output and offer Retry on recoverable lock contention; concurrent confirmation dialogs queue instead of being silently dropped; rapid branch selection is serialized so it can't duplicate terminals.
- **PR check status accuracy** — The PR detail popover no longer lists duplicate checks when a workflow re-runs (deduped to the newest run per name, matching the summary counts); `ACTION_REQUIRED` checks now count as failed so a PR blocked by a gate no longer renders as all-green; and the detail query fetches up to 100 checks to stay consistent with the summary tally.
- **MCP agent grid streaming** — Agent sessions spawned over MCP now register their grid watch channel and a terminal alias, so `format=grid` WebSocket streaming works.
- **MCP OAuth refresh tokens** — Authorization requests now include `offline_access`, so upstreams issue a refresh token instead of silently dropping to a re-auth prompt after the access token expires.
- **Binary terminal output** — The UTF-8 read buffer decodes iteratively (no reader-thread stack overflow on large binary reads), and absolute scrollback line reads return the correct rows.
- **AI Chat** — A lagging event stream no longer wedges the chat spinner; completion and error events still arrive.

## [1.4.2] - 2026-06-13

### Added
- **Nested terminal tabs** (#85) — Optional setting (Appearance ▸ Tabs, default off) that shows a branch's open terminals as a collapsible list under its sidebar row when the branch has more than one terminal, each with a status dot; clicking a sub-item switches to that terminal.

### Changed
- **Consistent HTML-preview search** — Find-in-page in the Preview tab's HTML view now uses the same SearchBar pill as the editor, diff, terminal and markdown views (with case/regex/whole-word toggles and a match counter), instead of a bespoke in-iframe overlay. The parent drives the iframe over a postMessage bridge; plugin panels keep their own overlay search.

### Fixed
- **Markdown search scrollbar ticks** — Find-in-page in the markdown view now shows the same match ticks on the scrollbar as the diff and editor views.
- **Approve button on your own PR** — The PR Approve button no longer briefly appears on your own pull requests before your GitHub identity has loaded (which GitHub rejects with a 422).
- **Branch indicator after fetch/rebase** — A momentarily-unreadable `.git/HEAD` (during a fetch, rebase or gc) no longer suppresses the next real branch-change refresh in the sidebar.
- **MCP upstream reconnection** — Fixed a rare crash when an OAuth upstream finished authenticating while being disconnected; upstreams added at runtime now reliably appear in the tool list, and a failed auto-connect no longer stalls the first tool listing.
- **Audio output device errors** are now logged instead of silently showing an empty device list.
- **Accessibility** — Terminal tab lists in the sidebar expose correct ARIA roles to screen readers.

## [1.4.0] - 2026-06-09

### Added
- **Native-feel smooth scrolling** — The terminal now scrolls with momentum and a draggable scrollbar, with selection highlight preserved across the animation.
- **Editor change-overview ruler** — VS Code-style colored ticks on the editor's scrollbar mark added/modified/deleted lines, fed by the same git-gutter data (no second diff parse).
- **Extended thinking in AI Chat** — Claude Opus 4.7+ reasoning is surfaced live in the chat panel.
- **Custom "Open in…" launchers** (#71) — Define your own editor/tool launch commands; iTerm2 and Tower are added as built-in launchers.
- **Event-driven smart prompts** — Repo watchers fire smart prompts on PR-pushed / PR-opened events behind an idle gate, instead of polling.
- **Markdown panel** — A dedicated markdown panel with refreshed keyboard shortcuts.
- **Sidebar "Create Branch"** — A Create Branch action with a condensed "Branch ›" submenu.
- **Faster terminal navigation** — Keybindings to return to the last terminal and jump to the next terminal awaiting input; the shortcuts panel now lists every canonical action.
- **Inline review comments on markdown ("tweaks")** — Attach review comments to any passage in a rendered markdown file; they're highlighted in place and stored inside the `.md` source as invisible HTML-comment markers that AI agents can read and apply.
- **Cmd/Ctrl+hover go-to-definition affordance** — Holding Cmd (macOS) / Ctrl underlines the symbol under the cursor in the code editor as a Cmd+Click affordance.
- **Cycle All Tab Types** (#58) — Optional setting (Appearance, default off) to let next/prev-tab cycle file/diff/markdown/editor tabs alongside terminals.
- **Right-click menu on terminal links** (#57) — Right-clicking a detected file path or URL offers Open and Copy link; single left-click still opens instantly.
- **JetBrains IDE family in the launcher** (#70) — IntelliJ IDEA, PyCharm, WebStorm, GoLand, CLion, PhpStorm, RubyMine, Rider, DataGrip, RustRover, Android Studio and Fleet are selectable as the default IDE and in "Open in…", launched with `--line`/`--column` goto.

### Changed
- **In-process git reads (gix)** — Branch detail, commit log/graph, ahead/behind, worktree paths, status counts, diff stats and blame now run through an in-process `gix` backend behind a GitReads port with a coalescing TTL cache, cutting git subprocesses and file-descriptor pressure. Narrow edge cases (sparse-checkout/submodule status, staged/commit diffs, renamed-file blame) fall back to the git CLI inside the adapter.
- **Diff & editor performance** (#020) — The diff parser moved to Rust; multi-file diffs and PR views are virtualized (only on-screen sections mount) and the editor's disk poll stat-gates before re-reading, keeping large diffs and long sessions responsive.
- **Commit graph** — Lane starts are marked and per-frame canvas reallocation was removed.
- **Error Log severity threshold** (#69) — Selecting a level shows that level and everything more severe (e.g. Warn shows Warn + Error), with a tooltip describing the range.

### Fixed
- **Readable terminal text on light themes** (#80) — The default foreground read an undefined CSS variable and fell back to gray; it now reads the theme's foreground color.
- **Consistent New Tab** (#81) — Cmd+T, the command palette, and File ▸ New Tab now route through the same handler.
- **macOS press-and-hold** (#79) — The accent-picker popup no longer hijacks key-repeat in the terminal.
- **Terminal interaction** — Right-click link menu works under mouse reporting, Cmd/Ctrl+C copies, and menu-bar Copy/Paste route to the focused terminal.
- **Freeze detection** — The freeze detector no longer reports paint jank or system sleep as UI freezes.
- **File browser** — Cross-repo copy/paste and keyboard focus fixed.
- **Shell & CLI** — The `tuic` sidecar resolves next to the executable (closes #52); zsh `compinit` runs correctly when the ZDOTDIR trick is skipped.
- **Worktrees** — Worktree-add failures are classified and fail loudly instead of creating phantom entries.

### Community
- [@jajugoguma](https://github.com/jajugoguma) — fixed terminal text being unreadable on light themes (#80)
- [@jklap](https://github.com/jklap) — added release-version checking to the macOS `install.sh` (#75)

## [1.2.11-nightly] - 2026-06-02

### Fixed
- **False completion sounds on wake (sleep/lid reopen)** — The silence timer's sleep-wake guard measured the inter-tick gap with `Instant` (monotonic), but on macOS `Instant` does not advance while the system is asleep, so the guard never fired and the wall-clock jump triggered a false busy→idle completion sound on every terminal. The gap is now measured against the same wall clock the idle decision uses.
- **Sleep counted as UI freezes** — The frontend freeze detector logged multi-minute "UI freeze" gaps on every lid reopen (RAF pauses during sleep). Gaps over 10s are now treated as system sleep and skipped, so the freeze count and logs reflect only real main-thread stalls.

## [1.2.10-nightly] - 2026-05-30

### Added
- **Full commit message body in expanded log view** — Expanding a commit in the Git Panel's Log tab now shows the complete multi-line commit message body below the subject; the subject is no longer truncated when expanded. Backed by a new `body` field on `CommitLogEntry`.

### Changed
- **GitHub OAuth scope reduced to `repo`** — Device Flow login no longer requests `read:org`. TUIC never calls any org/team/membership endpoint; access to org-owned private repos comes from `repo` + per-org SAML SSO authorization.
- **Bounded monitoring git fan-out** — Background repo-monitoring refreshes (`get_repo_summary`, `get_repo_structure`, `get_repo_diff_stats`) now share a concurrency semaphore (max 8 concurrent refreshes), so a `repo-changed` burst across many registered repos can't spike concurrent git subprocesses past the FD limit (EMFILE) or storm the main thread with IPC. Operational git (commit/push/stage/checkout/diff-on-click) is never throttled.
- **Faster grid-frame stuck recovery** — The stuck grid-frame back-off was shortened from 5s to 1s (chunked, respecting the running ticker) so the terminal recovers more quickly after a WebView JS-thread stall.

### Fixed
- **Raised file-descriptor soft limit at startup** — macOS launchd hands GUI apps a soft `RLIMIT_NOFILE` of 256; TUIC now raises it to 65536 at startup, preventing EMFILE errors during git fan-out bursts.
- **Dead terminal tabs no longer show green** — On PTY exit, the tab's shell state is now set to `exited` (it was left `idle` with a null session ID), so closed/dead tabs render the correct grey indicator instead of a green "active" dot.

## [1.2.9-nightly] - 2026-05-29

### Added
- **In-iframe Cmd/Ctrl+F search** — HTML previews and plugin panels now support find-in-page via an injected search overlay.
- **UI freeze detector** — A RAF-loop watchdog records main-thread stalls for diagnostics.
- **Programmatic main-window recovery** — If the window config is missing (bad edit/merge), the main window is now created programmatically so the app never starts headless.

### Fixed
- **Finder → File Browser drop coordinates (macOS Retina)** — Tauri reports drag-drop positions in logical (CSS) pixels on macOS; dividing by `devicePixelRatio` halved them on Retina and routed drops to the terminal behind the panel. Coordinates are now correct per-platform.
- **Duplicate plugin reload on Cmd/Ctrl+R** — The SDK and search scripts both posted `tuic:reload-request`, double-reloading plugin iframes. Deduplicated to a single handler.
- **Cross-platform release build** — Guarded macOS-only window-builder methods (`hidden_title`, `title_bar_style`) behind `cfg(target_os = "macos")` so Linux and Windows builds compile.

## [1.2.8-nightly] - 2026-05-29

### Added
- **Worktree setup scripts** — All three worktree creation paths (MCP `repo action=worktree_create`, HTTP `POST /worktrees`, HTTP `POST /sessions/worktree`) now resolve and execute the configured `setup_script` (per-repo override > global default) after creating the worktree. Result or error returned in the response as `setup_script` / `setup_script_error`.
- **Worktree-created event emission** — `worktree-created` Tauri event and `AppEvent::WorktreeCreated` event bus message now emitted from all three worktree creation paths (MCP, HTTP worktree, HTTP session+worktree), enabling frontend switch prompts.
- **AI Chat FileSandbox auto-init** — `run_conversation` now creates a `FileSandbox` from the session's CWD before tool dispatch, so file tools (`list_files`, `read_file`, etc.) work without a prior explicit sandbox setup.

### Fixed
- **IME candidate window positioning** — Hidden input element now tracks the terminal cursor position so East Asian IME candidate windows (Chinese Pinyin, Japanese, Korean) appear near the cursor instead of at the top-left corner of the screen. Also adds `compositionstart` handler for accurate position at composition onset. ([#42](https://github.com/sstraus/tuicommander/issues/42))
- **Git diff crash on deleted files** — `get_file_diff` no longer attempts `--no-index` for files deleted from disk, falling through to standard `git diff` which reads from the index.
- **Worktree stale directory** — Stale worktree directories now read the actual HEAD instead of echoing the originally requested branch; concurrent removal prevented via reentrancy guard. ([#47](https://github.com/sstraus/tuicommander/pull/47))

## [1.2.6-nightly] - 2026-05-25

### Added
- **Command block system** — Terminal output is segmented into command blocks (one per prompt+output cycle). Features include: semantic scrollbar marks with color-coded indicators, block timestamp overlay (hold `Ctrl+Cmd`), gutter click to select entire block output, block folding with `Cmd+Shift+.` toggle, block-scoped search with `Cmd+Shift+B` toggle, block navigation with `Cmd+Shift+Up/Down`, cap at 500 blocks per session (oldest evicted), and OSC 7770;block= agent-emitted block markers. Settings at `Settings > Terminal > Blocks`.
- **Heuristic agent-block detection** — Claude Code tool calls (`⏺ ToolName(args)`) are now detected heuristically and synthesized into AgentBlock start/end events, so the block system works without CC emitting OSC 7770 sequences.
- **Generators modal** — Secure value generators accessible from the command palette (`open-generators`). Generates: Password, UUID v4, UUID v7, ULID, CUID2, JWT Secret, TOTP Secret, Nano ID, Slug, Ed25519 Key Pair. All generation happens in the Rust backend via `ring` crate.
- **Process stats & monitor** — New MCP session action `process_stats` and HTTP routes (`/process/stats`, `/process/monitor`) returning CPU% and RSS memory for TUIC and all child process trees. The monitor route serves a self-contained HTML dashboard.
- **App logger extra fields** — Tracing events now capture all extra fields (beyond `message` and `source`) as a JSON `data` column, making structured logging queryable via the `/logs` endpoint.
- **Auto-standby** — SIGSTOP entire PTY process groups after configurable N minutes of being both unfocused and idle. SIGCONT on tab focus or agent message arrival. Settings at `Settings > General > Auto-Standby Timeout` (default 5 min, 0 = disabled). Pause badge in tab bar.
- **ANSI colors in markdown code blocks** — Code fences containing ANSI escape sequences are now colorized using `ansi-to-html` instead of being stripped.
- **Dormant repo throttling** — Cold repos (no active terminals) get 15s watcher debounce (vs 1.5s) and GitHub polling every ~10min with per-path jitter. Switching to a cold repo triggers immediate data refresh. New `set_hot_repos` command and `PUT /watchers/hot-repos` endpoint.
- **File browser intra-tree drag & drop** — Drag files and folders between directories within the file browser tree. Uses `renamePath` for the actual move.
- **Expanded menu bar** — New File, Find in Content, Clear Scrollback, Refresh Terminal, Maximize/Restore Pane, Focus Mode, Zoom All Terminals, File Browser, Outline, AI Chat, Compose, Global Workspace, Content Search, SSH Tunnels, Process Manager.
- **`prefers-reduced-motion` support** — Global CSS media query disables animations and transitions for users who prefer reduced motion.

### Changed
- **GitHub polling: updated_at change detection** — Replaced ETag-based HTTP caching with `updated_at` timestamp comparison, fixing stale data when GitHub CDN caches return 304 on changed content.
- **GitHub module split** — Extracted `github_debug` module for API debug logging. Fixed route prefix (`github` → `github-poller`). Removed dead Tauri commands.
- **PTY write error handling** — PTY writer `write_all`/`flush` failures are now logged via `tracing::warn` instead of silently ignored.
- **Session write tracing** — `write_pty` slash_mode logging downgraded from `info` to `trace` to reduce log noise.
- **Panel unmounting** — Outline, References, AiTriage, and Activity Dashboard panels now unmount when closed, releasing memos/subscriptions.
- **CanvasTerminal visibility** — Hidden terminals shrink canvas to 1x1px and clear caches. Cursor hidden when terminal is unfocused.
- **Dev build profile** — `[profile.dev]` opt-level=1 with dependencies at opt-level=2 for faster dev builds.

### Fixed
- **Idle keepalive spinner detection** — Tool progress spinners (◐◑◒◓) now detected for CC idle keepalive, preventing false idle transitions during tool execution.
- **MCP default pinned=false** — MCP-created tabs no longer default to pinned, preventing unintended tab persistence across branch switches.
- **Build: cfg-gate desktop modules** — Desktop-only modules properly gated for `tuic-remote` and agent-only builds.
- **CI: check-remote job** — New CI job catches missing `cfg(desktop)` gates before they break remote builds.
- **Block timestamp overlap** — `paintBlockTimestamps` now skips labels that would overlap vertically, preventing consecutive close prompts from rendering on top of each other.
- **Search scrollbar marks** — Orange markers for search match positions in the scrollbar, de-duplicated by pixel row, with cache key tied to search state.
- **Plugin watcher spurious reload** (#43) — Watcher now filters by `EventKind` (only Create/Modify(Data)/Modify(Name)/Remove), ignoring access/metadata events that caused ~1s flash loops on some Linux configurations. Disabled plugins are skipped entirely (zero IPC, zero store updates).
- **ContentIndex rebuild storm** — 60-second cooldown between consecutive index rebuilds for the same repo.
- **Memory caps** — Tool calls capped at 500, activity items at 500, PR notifications at 200 to prevent unbounded memory growth in long sessions.

## [1.2.3-nightly] - 2026-05-19

### Added
- **Tab ordering modes** — New Appearance setting with 3 modes: "Grouped by Type" (default, current behavior), "Terminals First" (terminals grouped left, non-terminals freely interleaved), and "Free" (any tab draggable to any position). Settings > Appearance > Tabs.

### Fixed
- **Clipboard soft-wrap** — Copying text from terminal no longer inserts spurious newlines at soft-wrap boundaries. The `WRAPLINE` flag is now respected during text extraction.

## [1.2.2-nightly] - 2026-05-13

### Added
- **MCP panel screenshot** — MCP agents can capture plugin panel content as WebP images via `ui(action=screenshot, id=<pluginId>)`. Enables agents to visually verify rendered HTML dashboards.
- **Iframe keyboard shortcut forwarding** — TUIC keyboard shortcuts now work when an iframe (plugin panel, HTML preview) has focus. A key forwarder re-dispatches matching shortcut events to the parent document.
- **Bracketed paste mode** — Terminal paste now respects the application's bracketed paste mode flag. Escape sequences are only wrapped when the terminal app has activated bracketed paste, fixing raw escape bytes leaking into apps like `psql` password prompts.
- **Git log time-of-day** — Commits within 7 days now show "1d ago · 14:32" instead of just "1d ago". Hover shows the full timestamp. Helps distinguish same-day commits when deciding what to revert.
- **Terminal text selection API** — New `terminal_get_selection_text` Tauri command extracts text from a row/column range in the terminal grid.
- **Double-click word selection** — Double-clicking in the terminal now includes hyphens (`-`) and underscores (`_`) in word boundaries, matching common shell identifiers.
- **Preview tab Edit button** — Preview tab header now has an Edit button (pencil icon) that opens the file in the built-in code editor.
- **Native drag to external apps** — Drag files from the File Browser to Finder, email clients, and other external applications using OS-level drag via `tauri-plugin-drag`.
- **Mermaid diagram rendering** — Fenced ` ```mermaid ` code blocks in markdown tabs are rendered as interactive SVG diagrams. Mermaid.js is lazy-loaded on first use with dark theme and strict security.
- **Group park/unpark** — Park or unpark all repositories in a sidebar group at once via context menu or command palette.
- **Status bar pulse animation** — Status messages (e.g., "Copied to clipboard") flash with a single 600ms color pulse to attract attention.
- **Per-repo PR visibility filters** — Override global draft/conflicting/CI-failing PR filter settings per repository. Tri-state toggle (Show / Default / Hide) in repo settings under "PR Visibility".
- **TriStateToggle component** — Reusable 3-position toggle (hide / default / show) matching existing pill-switch style, used for per-repo PR filter overrides.
- **`terminal_hyperlink_span`** — New Tauri command that returns the full contiguous span of an OSC 8 hyperlink at a given cell, enabling correct hover underlines across the entire link.

### Fixed
- **Dictation to CanvasTerminal** — Transcribed text from dictation is now correctly routed to the PTY when using CanvasTerminal, instead of silently dropping.
- **Notes panel font size** — Notes panel text, empty state, and input aligned to `--font-sm` (12px) to match GitPanel and other side panels.
- **Compose panel text persistence** — Text in the compose panel is preserved when closing and reopening the panel, instead of being lost.
- **External API hidden from agent menu** — The "External API" entry no longer appears in the "Add Agent" sidebar submenu since it's not a CLI agent.
- **Terminal scroll-to-bottom on click** — Clicking in a scrolled-up terminal no longer snaps to bottom. Mouse events (SGR reports, focus/blur) now use `writePtyNoScroll()` which writes to the PTY without resetting scroll position.
- **OSC 8 hyperlink hover underline** — Hover underline now spans the full link instead of a single character, using the new `terminal_hyperlink_span` backend API.
- **OSC 8 `file://` URI links** — Clicking `file:///path` links now correctly opens the file by stripping the `file://` prefix before path resolution.
- **OSC 8 tilde path resolution** — OSC 8 hyperlinks with `~/` paths now resolve through `resolve_terminal_path`, expanding the home directory before opening.
- **Offscreen selection copy** — Copying terminal text that extends beyond the visible viewport now delegates to the backend, which has full scrollback access. Previously, offscreen rows were silently dropped.
- **Auto-heal CI toggle style** — Replaced native HTML checkbox with pill-switch toggle matching the CI check item row style (proper padding, hover, alignment).
- **Vitest localStorage shim** — Test setup now handles `localStorage` being fully undefined (not just missing `.clear()`), fixing 524 test failures in environments without `--localstorage-file`.

### Changed
- **Warp agent type removed** — Warp as an AI agent type has been removed from the agent configuration, detection, and documentation. Warp as a terminal application (Open in Warp) remains supported.
- **OpenCode env override** — OpenCode agent now supports `OPENCODE_BIN` environment variable to override binary detection.
- **CSS modules migration** — Migrated QuitDialog, Dropdown, StatusBadge, ZoomIndicator, PanelResizeHandle, TerminalArea, and StatusBar from global CSS classes to scoped CSS modules. ~470 lines of dead global CSS removed.
- **Markdown CSS extraction** — All `#markdown-content` styles moved from the monolithic `styles.css` to a dedicated `markdown-content.css` file next to ContentRenderer. Dead CSS sections cleaned from styles.css (~200 lines removed).
- **Cell width rounding** — `measureFont()` now uses `Math.round()` instead of `Math.ceil()` for cell width calculation, producing tighter character spacing.

### Community
- [@paulovitin](https://github.com/paulovitin) — fix git worktree hook path resolution (#36)
- [@paulovitin](https://github.com/paulovitin) — close orphan worktree terminals on removal (#37)
- [@paulovitin](https://github.com/paulovitin) — replace RSA with direct ECDSA for key exchange (#38)
- [@paulovitin](https://github.com/paulovitin) — extract `nextDefaultName()` to eliminate tab naming duplication (#35)
- [@paulovitin](https://github.com/paulovitin) — PR visibility filters + viewer PR guarantee (#34)
- [@paulovitin](https://github.com/paulovitin) — fix zsh compinit with ZDOTDIR trick (#32)

## [1.2.0] - 2026-05-10

### Added
- **SSH tunnel manager + remote connections** — Full remote access layer: SSH tunnel supervision with auto-reconnect, slim `tuicommander-remote` headless binary for remote machines, and desktop connection manager UI. Includes protocol versioning, health checks, token rotation, TLS configuration, and rate-limited auth.
- **CSV/TSV/PSV/SSV file preview plugin** — Opening tabular data files (.csv, .tsv, .psv, .ssv) in the file browser shows a sortable HTML table with sticky headers, rainbow column tinting, and an "Edit" button to open in CodeMirror. Plugin API: `host.registerFilePreview()` and `host.openEditorTab()` with new `"ui:file-preview"` capability.
- **"What's New" dialog** — AI-generated release notes shown once after stable version updates, with community contributor attribution. `make bump` now auto-generates notes via Claude CLI with interactive approval.
- **Nightly rolling changelog** — CI generates changelog from git log since last stable tag for nightly releases.
- **AI Triage feature flag** — AI-powered diff triage is now an experimental sub-feature that can be independently toggled under Settings > General > Experimental Features.
- **AI Watchers feature flag** — Terminal watchers are now an experimental sub-feature with independent toggle. The watcher toolbar button is hidden when disabled.
- **Watcher cooldown field** — Configurable minimum seconds between consecutive watcher fires, with a `?` tooltip explaining the setting.
- **Reusable settings components** — Extracted `SettingToggle`, `SettingSelect`, `SettingSlider`, `SettingInput` to reduce duplication across all settings tabs.
- **AI Prompts save/revert** — Explicit Save and Revert to Default buttons replace the implicit on-blur auto-save behavior.
- **Code editor enhancements** — Undo/redo history, code folding, auto-close brackets, scroll past end, block selection (Alt+drag), drop cursor, special character highlighting, and CSS color preview swatches (`@replit/codemirror-css-color-picker`).
- **Blog post: beta features tour** — Overview of SSH tunnels, tuicommander-remote, AI Triage, AI Watchers, and AI Chat with instructions to enable and a call for feedback.

### Fixed
- **GitHub badge always visible** — Sidebar GitHub badge now hidden correctly without `:has()` CSS support.
- **Tab URL resolution** — Plugin tabs with `file://` URLs resolved via IPC instead of direct iframe src. MCP session root used for repo-scoped tab URLs.
- **Watcher session ID mismatch** — Watcher attach was passing frontend tab IDs instead of PTY session UUIDs, causing watchers to never trigger. Now correctly uses the PTY session UUID.
- **Watcher update validation** — `update_rule` now delegates to `validate_rule` after applying partial updates, eliminating duplicated inline checks and inconsistent error messages.
- **Search input autocorrect** — Disabled `autocomplete`, `autocorrect`, and `spellcheck` on all search/filter inputs (11 components). macOS WebKit was autocorrecting search queries.
- **Dead-key composition on macOS** — Fixed dead-key input (accents, quotes) not working in macOS WKWebView terminals. ([#31](https://github.com/sstraus/tuicommander/pull/31) by [@paulovitin](https://github.com/paulovitin))

### Community
- [@paulovitin](https://github.com/paulovitin) — dead-key composition fix (#31)

## [1.1.3] - 2026-05-06

### Added
- **Cached git helpers for MCP** — `get_repo_info_cached`, `get_github_status_cached`, `get_worktree_paths_cached` avoid redundant git subprocess spawns in synchronous MCP handlers.
- **Content index in-flight guard** — `DashSet`-based dedup prevents concurrent index rebuilds for the same repo on rapid `RepoChanged` events.
- **Worktree switch agent notification** — When an agent is running, worktree switch dialog offers "Move & Notify" which reassigns the terminal to the new branch and sends a message to the agent instead of blindly `cd`-ing.

### Fixed
- **Terminal search broken** — `canvasTerminalRef` in `Terminal.tsx` was a plain `let` variable, invisible to SolidJS reactivity. `TerminalSearch` always saw `undefined` and silently returned no results. Converted to `createSignal`.
- **Worktree `git worktree prune`** — Removed automatic prune from `get_worktree_paths` (was running on every call, including cached paths; prune is a write operation that shouldn't happen in a read path).

## [1.1.2] - 2026-05-06

### Added
- **Contributing guide** — `CONTRIBUTING.md` with test requirements, quality gates, PR guidelines, and common rejection reasons.
- **Parallel GitHub ETag filtering** — `filter_changed_repos` now fires all ETag requests concurrently via `join_all`, reducing poll latency for multi-repo setups.
- **Browser-mode dictation routes** — HTTP endpoint mappings for dictation, notification sound, relay status, update channel, and `detect_all_agent_binaries`. Dictation settings tab hidden in browser mode.
- **Terminal link underlines** — Detected links always show dashed underlines; solid underline on hover.

### Changed
- **Scroll reverted to progressive acceleration** — Replaced inertia-based scrolling with previous progressive acceleration approach.

### Fixed
- **`has_custom_settings()` completeness** — Now includes `mcp_upstreams` and `branch_labels` fields, preventing premature config pruning.
- **`remove_branch_label` silent failure** — Save errors now logged via `tracing::warn!` instead of being silently dropped.
- **GitHub ETag error logging** — Network errors during ETag checks now logged via `tracing::debug!` for debugging.
- **Branch label cleanup on worktree deletion** — Frontend store now clears the label when a worktree is removed, preventing stale labels.
- **Link detection regex sharing** — `checkLinksAtRow` hover path reuses hoisted regex constants instead of allocating new instances per call.
- **GitHub poller clippy fixes** — Replaced manual `% == 0` with `is_multiple_of()`, extracted `PollMutableState` struct to reduce `poll_batch` argument count.

## [1.1.1] - 2026-05-05

### Added
- **CLI companion (`tuic`)** — Standalone sidecar binary for controlling TUICommander from the terminal. Supports file opening with goto (`tuic file.rs:42`), session management (ls/new/kill/send), agent orchestration (spawn/ls/send), and tmux compatibility mode (`tuic alias`). First-run install prompt on app launch; install/uninstall from Settings > General. Auto-updates on startup.
- **Scrollback reflow setting** — New `scrollbackReflow` option (Settings > General) enables history-only reflow on column resize. Keeps scrollback readable when side panels narrow the terminal, without affecting cursor-addressed TUIs on the visible screen. Backed by new `ReflowMode::HistoryOnly` in the alacritty grid fork.
- **Delta cursor for MCP read tools** — `session action=output`, `read_screen`, and `drive_agent` now return a `cursor` field. Pass `since_cursor` on subsequent calls to receive only new scrollback lines, avoiding full re-reads.
- **`drive_agent` MCP tool** — Atomic send→wait→read operation for external agents. Sends a command, waits for idle/pattern match, returns screen + shell state + cursor in a single call.
- **Session aliases** — Human-friendly aliases for terminal sessions derived from repo directory name (e.g. `tuicommander` → `tc-1`). All `ai_terminal_*` tools accept aliases in place of UUIDs. Visible in tab tooltips and `list_sessions` output. Counters reset on app restart.
- **AI Prompts editor** — Customizable system prompts for AI services (starting with Diff Triage). Collapsible section in Settings > Agents with textarea, "Modified" indicator, and "Reset to Default" button. Prompts stored in `ai-prompts.json`, exposed via MCP (`config` tool: `list_ai_prompts`, `load_ai_prompt`, `save_ai_prompt`). Smart prompts also exposed via MCP (`list_prompts`, `load_prompt`, `save_prompt`).
- **PR context variables** — Extracted `prContextVariables()` utility for reuse across SmartButtonStrip in PR detail popover and GitHub panel sidebar.
- **Plugin tab "Open in Browser"** — Right-click context menu on plugin panel tabs now includes "Open in Browser" action.
- **New tips** — Three new tips for the `tuic` CLI (general, tmux alias, file open).

### Changed
- **Editor tabs: worktree-aware fsRoot** — `EditorTabData` now carries a separate `fsRoot` field for file I/O, distinct from the canonical `repoPath`. Fixes file reads in git worktrees where the on-disk path differs from the repo root.
- **GitHub poller interval** — Base polling interval increased from 30s to 60s; cache TTL aligned. Reduces API call volume.
- **MCP GitHub tools use poller cache** — `prs` and `status` actions read from the poller's cached PR data instead of making live GitHub API calls, eliminating fan-out requests.
- **IDE launcher: open -a fallback on macOS** — `open_in_app` for VS Code, Cursor, and Windsurf now falls back to `open -a <App>` when the CLI binary isn't in PATH, using a shared `goto_editor_cmd()` helper (DRY refactor).
- **IDE launcher hover CSS** — Fixed hover style applying to disabled split buttons.
- **Sidecar builder** — `build-sidecar.mjs` now builds multiple sidecars (tuic-bridge + tuic CLI), with incremental skip when source hasn't changed.
- **Terminal touch: pixel-based momentum scrolling** — Touch scroll now uses pixel-level accumulation with velocity-based momentum and friction, matching native feel. Replaces line-granularity scroll.
- **Canvas terminal: 6px left gutter** — Added a left gutter for error markers and visual breathing room, with proper coordinate adjustments for mouse position, selection, and painting.
- **Canvas terminal: per-glyph rendering** — Replaced ligature-batched `fillText` runs with per-glyph rendering. Eliminates sub-pixel cursor drift on long lines caused by `Math.ceil` cell width accumulation. Removed PUA special-case (Nerd Font icons now render via normal path).
- **Canvas terminal: progressive scroll acceleration** — Wheel and touch scroll now use pixel accumulation with progressive acceleration. First screenful is damped (0.5x), then ramps up. Prevents scroll overshoot at start while allowing fast scrolling.
- **Alt-screen recovery** — Agent→shell transition now uses `terminal_exit_alt_screen` IPC (display-side injection) instead of writing escape sequences to PTY stdin, preventing leaked input.
- **VtLogBuffer: alt-screen query + history reflow** — Added `is_alternate_screen()` and `set_reflow_history()` methods to VtLogBuffer for display-side alt-screen detection and history-only reflow configuration.

### Fixed
- **OSC 9;4 progress: last match wins** — `parse_osc94` now uses `captures_iter().last()` to report the most recent progress value when multiple OSC 9;4 sequences arrive in a single PTY chunk.
- **Clipboard status message restored** — `copySelection()` in CanvasTerminal now shows "Copied to clipboard" / "Copy failed" in the status bar, lost during xterm→canvas migration.
- **Cargo config** — Fixed `term.progress = true` placed under `[env]` section (invalid), moved to `[term]` section as `progress.term-integration = true`.
- **Terminal.tsx disposal guards** — Added `disposed` checks in `handlePtyData`, `handleParsedEvent`, `onData`, OSC 133, and title listeners to prevent updates after component unmount.
- **Collapsible if (clippy)** — Collapsed nested `if` statements in PTY ACK flush path.
- **MCP `load_prompt` response shape** — Fixed `to_json_or_error(Ok::<_, String>(...))` wrapping response in `{"Ok": {...}}` instead of flat fields.
- **MCP `save_ai_prompt` partial save** — Changed from constructing a fresh `AiPromptsConfig` to load-modify-save pattern, preventing future field wipe when new services are added.
- **Enrichment settings: duplicate label** — Removed duplicate "Enrichment" label rendered by both parent group and `SlotRow` component. Added `showLabel` prop to `SlotRow`.
- **Enrichment hint visibility** — "Rate-limited to ~10/min" hint now only shows when enrichment is toggled on.
- **CSP connect-src** — Added `plugin:` scheme to `connect-src` directive for Tauri plugin fetch requests.

## [1.1.0] - 2026-05-04

### Added
- **Provider Registry** — Centralized multi-provider configuration system. Define providers (Anthropic, OpenAI, OpenRouter, Ollama, custom OpenAI-compatible) with per-provider API keys and model lists. Slot resolver maps logical slots (`headless`, `chat`, `triage`) to concrete provider+model pairs with a fallback chain. Legacy `ai-chat-config.json` settings auto-migrated to `providers.json` on first load. New `Credential::Provider` variant stores keys in OS keyring. Settings > Providers tab with full CRUD UI for providers, models, and slot assignments. Rust consumers (`ai_chat`, `ai_agent`, `headless`, `triage`) resolve models via the registry instead of reading config directly.
- **AI Diff Triage** — Holistic LLM-powered diff review panel. Shows `git diff` changes grouped by file with progressive loading. Heuristic pre-classification (formatting, rename, test, config) runs locally before sending to the LLM. Multi-turn conversation via `TriageSession` struct allows follow-up questions about specific findings. Diff button on individual findings opens the relevant file diff. `classify_multi_turn` wired into `run_diff_triage` with refresh support from the frontend.
- **Scrollback History Overlay** — Experimental read-only overlay for viewing full terminal scrollback beyond the visible buffer. Gated behind `scrollHistoryEnabled` settings flag. Built with LogSpan ANSI reconstruction from `VtLogBuffer`. Features: selection with auto-copy, `::selection` highlight, `Cmd+F` search with match highlighting, theme-synced ANSI CSS variables, grid-aligned positioning.
- **Generic detachable panel system** — Any panel can be detached to a separate window via `open_panel_window` Rust command. Two-tier sync: self-sufficient panels call Rust directly, projection panels receive state snapshots via `emitTo`. Panel adapters define per-panel serialization, actions, and components. Shared `PanelWindowControls` component replaces per-panel detach/close buttons. Generic lifecycle functions (`togglePanel`, `detachPanel`, `reattachPanel`) replace per-panel callsites.
- **Activity Dashboard detach** — Detach button in Activity Dashboard header and "Open Activity Dashboard in separate window" Command Palette entry. Live state sync at 1 Hz via PanelSyncProvider/Receiver.
- **Git Panel detach** — Git Panel can now be detached to a separate window via the panel header button or Command Palette.
- **Tab detach** — Right-click any tab → "Detach to Window" opens it in a floating OS window. PTY session stays alive; closing the floating window returns the tab to the main window.
- **GitHub PR/issues polling migrated to Rust** — Background PR and issues polling moved from TypeScript intervals to the Rust backend for reliability and lower overhead.
- **GitHub API rate limit optimization** — Smarter API call batching to reduce rate limit consumption.
- **Drag files from FileBrowser** — Drag files from the File Browser panel onto the terminal area or tab bar to paste shell-quoted paths.
- **Notes enhancements** — Pending count badge, "Clear completed" action, batch asset delete for cleaned-up notes.
- **Dictation autoSend** — When enabled, dictation transcription is automatically submitted via `sendCommand()` with sessionId safety.
- **Daily-rotated log files** — App logs now rotate daily with automatic cleanup of old log files.
- **CI release automation** — Auto-publish GitHub release after finalize step.

### Changed
- **`uiStore.detachedPanels`** — Replaced `aiChatDetached: boolean` with generic `detachedPanels: Record<string, string>` map. `isDetached(panelId)` / `setDetached()` / `clearDetached()` replace per-panel flags.
- **Terminal perf: RAF-coalesced painting** — All paint triggers (frame arrival, keydown, mousedown) now go through a single `scheduleRepaint()` via `requestAnimationFrame`, eliminating synchronous double-paints.
- **Terminal perf: zero-clone frame dispatch** — `send_grid_frame` skips the ~121KB frame clone when no WebSocket subscribers are connected (common desktop case).
- **Terminal perf: single screen snapshot per PTY chunk** — Chrome cutoff detection uses a borrowed `&[String]` view; downstream parsers share one owned snapshot instead of re-acquiring the lock.
- **Terminal perf: in-place string trim** — `read_screen_text()` and `row_to_text()` trim trailing whitespace via `truncate()` instead of allocating a new String per row.
- **Panel routing extracted** — `panelRouter.tsx` provides `registerPanel()`, `renderPanelMode()`, and panel adapter registry, replacing hardcoded panel routing in App.tsx.
- **AI Chat migrated to generic panel lifecycle** — `open_ai_chat_window` replaced by `open_panel_window("ai-chat", ...)`. Channel-based streaming sync preserved.
- **AltScreenHistory rewritten** — Replaced custom DOM renderer with a native read-only terminal overlay. Dead DOM-rendering CSS stripped.

### Fixed
- **Suggest overlay z-index** — Raised overlay above active terminal pane to prevent clipping.
- **Provider schema migration** — Fixed reactivity and error handling during legacy config migration to provider registry.
- **Editor unlock for external files** — External files opened via `tuic://open/` can now be unlocked for editing.
- **Stale agentSessionId** — Prevented stale session IDs from resuming the wrong agent session on branch switch.
- **Plugin panel refocus** — Stopped plugin panels from stealing terminal focus after tab switch.
- **tuic://open/ paths** — External paths opened via deep link now correctly open as read-only editor tabs.
- **Panel mode early-return** — Fixed early return in panel mode that skipped cleanup.
- **WebGL atlas rebuild** — Atlas now rebuilds on font change and DPR change to prevent glyph corruption.
- **GitHub CheckSummary** — Recomputed from fresh check details on popover open instead of using stale cached summary.
- **Git lock contention** — Resolved `resolve_context` fan-out causing `index.lock` contention; optimized variable resolution.
- **Alt-screen history guards** — Added safety guards for edge cases in scroll history overlay.
- **Rust safety** — `index.lock` age guard, `vt_log` lock scope reduction, `saturating_mul` overflow protection, `let-chains` for nested `if-let` patterns, `is_some+unwrap` replaced with `if-let` per clippy.
- **ANSI CSS variable sync** — Scroll history overlay now syncs CSS variables with the active terminal theme.
- **Scroll history grid alignment** — Overlay correctly aligns with terminal grid metrics.
- **Session conflict dedup** — Deduplicated session-conflict events and regenerate `tuicSession` in-memory.
- **Network interface display** — Settings now shows network interface and QR code when remote access is enabled.
- **Panel beforeunload** — JS `beforeunload` handler for OS window close + reattach reopens overlay correctly.
- **License correction** — Fixed license from MIT to Apache 2.0 in README and website footer.

### Removed
- `open_ai_chat_window` Rust command (replaced by generic `open_panel_window`)
- `aiChatDetached` field in uiStore (replaced by generic `detachedPanels` map)

## [1.0.7] - 2026-04-24

### Added
- **Manual MCP configuration panel** — Expandable "Manual MCP configuration" section in Settings > Services > TUIC Tools shows the `tuic-bridge` binary path and a ready-to-paste JSON snippet for manual MCP client setup. Copy button included.
- **Copy-on-select settings toggle** — UI toggle in Settings > General > Terminal section to enable/disable copy-on-select behavior (previously only in Appearance tab).
- **Copy feedback for Cmd+C** — The `handleCopy` capture-phase listener (Cmd+C) now shows "Copied to clipboard" in the status bar, matching the existing behavior of copy-on-select and Ctrl+C paths.

### Fixed
- **Terminal copy trailing whitespace** — All copy paths (Cmd+C, Ctrl+C on Windows, copy-on-select, keyboard shortcut handler) now strip trailing spaces that xterm.js pads to the terminal width.
- **Windows path support across frontend** — Replaced all raw string path operations (`startsWith("/")`, `.split("/").pop()`, template literal joins) with cross-platform `pathUtils` helpers (`isAbsolutePath()`, `pathBasename()`, `joinPath()`, `pathStartsWith()`, `pathStripPrefix()`). Affects 15+ files: App.tsx, FileBrowserPanel, CodeEditorTab, MarkdownTab, HtmlPreviewTab, PaneTree, ChangesTab, resolveTuicPath, planPlugin, mdTabs, diffTabs, editorTabs, useAppInit, useFileDrop, useGitOperations.
- **Git path traversal validation** — Simplified `validate_paths_within_repo` to use depth-counting instead of full path canonicalization, correctly rejecting `../` escapes without needing the file to exist on disk.
- **`$HOME` restriction removed from `list_markdown_files` and `read_file`** — The artificial `$HOME` boundary blocked access to repos on external drives and non-standard locations. The user IS the trust boundary for a local desktop app.
- **Session conflict flag-file approach** — Replaced `maybe_reset_tuic_session` (which wrote `export TUIC_SESSION=...` directly to the PTY) with a flag-file mechanism. Now creates `no-session-inject.$TUIC_SESSION` in the config dir; shell wrappers (zsh/bash/fish) check for this file before injecting `--session-id`. Eliminates PTY writes that could corrupt TUI output.

### Added (tests)
- **pathUtils comprehensive test suite** — 40+ tests covering `isAbsolutePath`, `normalizeSep`, `pathStartsWith`, `pathStripPrefix`, `joinPath`, `pathParts` with Unix, Windows, UNC, and mixed-separator cases.
- **Windows path tests for planPlugin, mdTabs** — Drive-letter CWD resolution, mixed separators, absolute path preservation.

## [1.0.6] - 2026-04-18

### Added
- **Run config Edit/Delete menu** — The Delete button on each run config in Settings → Agents has been replaced with a `···` dropdown containing Edit and Delete. Edit opens an inline form for name, command, and args (with cross-agent duplicate name validation). Env editing continues to live on the dedicated "Env" button.
- **Auto-inject session binding for Claude Code and Goose** — Shell integration (zsh, bash, fish) wraps `claude` and `goose` commands with functions that transparently inject session identifiers (`--session-id $TUIC_SESSION` for Claude, `--name $TUIC_SESSION` for Goose `session`/`run` subcommands), ensuring deterministic 1:1 tab↔session mapping. Wrappers are bypassed when the user explicitly passes session/resume flags.
- **Goose CLI agent support** — Full integration for Block's Goose CLI: foreground process detection, status line spinner parsing (`(Ctrl+C to interrupt)` pattern), session-aware resume via `--name`, MCP client identification, Settings panel entry, and agent icon/badge.
- **Claude Wakeup plugin** — Agent-scoped external plugin (`plugins/claude-wakeup/`) that wakes Claude Code when it stalls without asking a question. Sends a verification prompt after 20 s of idle, detects "done" replies via busy-cycle duration (short <8 s = done, long ≥8 s = continued), disarms until next user turn. Typing suppression: every busy→idle transition resets the idle clock, preventing false wakes during keystroke-generated shell-state blips. Max 3 wakes per stall, 12 per session. Markdown stats dashboard. Configurable thresholds via `data/config.json`.
- **Agent cron scheduler** — Time-triggered agent tasks with cron expressions. Define jobs (cron + goal) in Settings > AI Chat > Scheduler. Persisted to `ai-cron.json`, ticks every 30 s. Tauri commands: `load_scheduler_config`, `save_scheduler_config`.
- **Agent model overrides per task phase** — Route different models to different tool phases (`plan`, `search`, `read`, `write`) in the AI Agent loop. Configure in Settings > AI Chat. Stored as `agent_model_overrides` in `ai-chat-config.json`.
- **PTY orchestration tools for multi-agent control** — Swarm agents can spawn, monitor, and coordinate PTY sessions via MCP `agent` and `session` tools.
- **Cross-session memory injection** — `build_cross_session_section()` scans prior sessions in the same repo and injects a summarised memory block into the agent system prompt.
- **`search_tools` / `call_tool` MCP bridge** — Speakeasy lazy-discovery meta-tools (`search_tools`, `get_tool_schema`, `call_tool`) replace the full tool list when `collapse_tools` is enabled, cutting MCP context from ~35k to ~500 tokens.
- **Unsafe mode** — Lock icon in the AI Chat header toggles `TrustLevel::Unrestricted`, bypassing approval prompts and sandbox. Confirmation dialog + red header indicator.
- **Agent cost tracking UI** — Live usage footer in AI Chat: prompt tokens (↑N), completion tokens (↓N), estimated cost ($X.XXXX), cache hit rate.
- **`search_code` BM25 tool** — Semantic search over repo files via `content_index`, available as the 13th agent tool and via `ai_terminal_search_code` MCP.
- **AI Chat conversation history panel** — Slide-in list of saved conversations with title, terminal name, message count, date. Click to load.
- **AI Chat per-terminal state** — Each terminal maintains independent chat history, streaming state, and conversation ID (keyed by `tuicSession`). Frozen-state banner when no terminal is focused.
- **AI Chat detachable panel** — Detach the AI Chat panel into a separate Tauri window for multi-monitor workflows. Click the detach icon in the panel header to pop it out; the main window shows a placeholder with a "Bring back" button. Cross-window state sync via a Rust-side ChatRegistry using `Channel<ChatEvent>` fan-out (no `app.emit` for high-frequency data). The detached window shares the same conversation state — streaming chunks, messages, and errors are projected in real-time.
- **Refresh terminal** (`Cmd+Shift+L`) — Rebuilds the terminal renderer to fix corrupted WebGL glyphs without clearing content.
- **Tab drag reorder for all tab types** — Non-terminal tabs (diff, editor, markdown, plugin panels) can now be reordered via drag-and-drop.
- **`get_input_buffer_content` Tauri command** — Read the terminal input line buffer; whitelisted for plugins with `pty:read` capability.
- **Keyring warm-up** — Bearer tokens cached in memory after first keyring read, eliminating repeated macOS Keychain prompts.

### Changed
- **Session restore filters plain shell tabs** — On restart, only terminals with an active agent session (`agentType` set) are restored. Plain shell tabs are discarded and a fresh terminal is spawned, eliminating ghost tabs that couldn't resume anything.

### Fixed
- **Sidebar "Add Agent" launch now respects run configs** — Launching an agent from the branch context menu previously wrote the raw command string directly to the PTY via `pty.write(cmd + "\r")`, bypassing `sendCommand()` and missing the Ctrl-U prefix required by Ink-based agents. Now routes through `pty.sendCommand()` like the active-terminal path.
- **Intent parsing for custom command aliases (C2, c, wrappers)** — `classify_agent` only matches literal binary names like `"claude"`, so sessions launched via aliases, symlinks, or wrapper scripts never flipped `agent_active_for_parse` on and silently dropped `intent:`/`suggest:` tokens. `PtyConfig` now carries an optional `agent_type` that pre-seeds `SessionState.agent_type` at PTY creation, and `get_session_foreground_process` falls back to the pre-seeded type when it sees a non-shell process that `classify_agent` doesn't recognise.
- **`--session-id` injection for custom Claude commands** — `buildAgentLaunchCommand` now accepts an `agentType` parameter and injects `--session-id` for any command the user has mapped to the Claude agent type, not only commands whose binary name starts with `claude`.
- **Resume session uses the default run config** — Clicking resume in the top bar previously hardcoded `claude --resume <id>`, ignoring whatever custom command/args the user configured as their default run config. `buildResumeCommand` / `verifyAndBuildResumeCommand` now swap the binary for `runConfig.command` and append `runConfig.args` after the resume flag (e.g. `c2 --resume <id> --model claude-opus-4-6`).
- **Activity Dashboard stale row content** — The dashboard snapshotted full row data (intent, status, last-prompt, task) every 10 s, so intent/status updates lagged up to 10 s behind reality. Now the snapshot stabilises only the sort order (list of ids); row contents are read live from the store on each render.
- **Context menu stealing terminal focus** — Right-clicking a panel (e.g. File Browser "Copy Path") left focus on the panel after the menu closed, making the terminal unresponsive to keystrokes. The context menu now captures `document.activeElement` on open and restores focus on close.
- **AI Chat message list compression** — Chat panel content was visually compressing instead of scrolling when many messages were present. Fixed with `min-height: 0` on the flex container and `flex-shrink: 0` on children.
- **15 Clippy errors from Rust 1.95** — Fixed `unnecessary_sort_by` (→ `sort_by_key` with `Reverse`), `collapsible_match` (→ match guards), and `collapsible_if` (→ `&&` chains) across 10 files.
- **Cmd+modifier+Enter sending `\r` to PTY** — Prevented Cmd+Shift+Enter and Cmd+Alt+Enter from injecting carriage returns into the terminal.
- **MCP stdio proxy "0 tools"** — `StdioMcpClient.rpc()` now matches JSON-RPC responses by `id`, skipping server notifications emitted between request and response. Previously a single interleaved notification (e.g. `notifications/tools/list_changed`) would be consumed as the `tools/list` response, silently yielding 0 tools.
- **macOS keychain prompt spam** — `HttpMcpClient` now caches the resolved bearer token in memory after the first keyring read. Health checks (every 60s) and tool calls reuse the cache instead of hitting the OS keychain each time. Cache is invalidated on 401 and re-populated after token refresh.
- **Tilde expansion in all user-supplied paths** — `std::process::Command` and `std::fs` do not expand `~`; paths like `~/bin/mdkb` failed with ENOENT. Added `crate::cli::expand_tilde()` and applied it in 11 sites: MCP stdio client (command, args, cwd), PTY (shell, cwd), agent spawn (binary_path, cwd), headless prompts (command, args, repo_path), shell scripts (repo_path), MCP HTTP session/agent/transport (cwd), worktree setup scripts (cwd), plugin exec (cwd validation), and plugin filesystem (path validation).
- **Silent "0 tools" diagnostic** — Both stdio and HTTP MCP clients now log `warn!` when `tools/list` response is missing `result.tools`, instead of silently returning an empty tool list via `unwrap_or_default()`.

### Added (tests)
- **Claude Wakeup unit tests** — 18 tests covering `canWake` state machine, typing-resets-lastIdleAt (the main false-wake bug), done detection via busy-cycle duration, and re-arm after disarm logic.
- **Stdio RPC id-matching tests** — Two tests verifying that `rpc()` correctly skips interleaved server notifications (1 and 3 notifications) and returns the matching `tools/list` response.
- **`expand_tilde` unit tests** — Tests for `~/path` expansion and no-op cases (absolute paths, relative paths, `~other_user`).

## [1.0.6] - 2026-04-18

### Added
- **AI Chat knowledge history overlay** (`#1387-e745`) — Two-pane modal browser for persisted `SessionKnowledge`: sessions list (sorted by most recent activity) + per-command detail pane. Debounced full-text search matches command, output snippet, inferred `error_type`, and opt-in `semantic_intent`. Filters: errors-only checkbox, date window (24h / 7d / 30d / all). Per-command card shows kind badge, timestamp, CWD, exit code, duration, output snippet, and a copy-command button. Launched from a "History" button next to the `SessionKnowledgeBar`; Esc closes. Backed by two Tauri commands: `list_knowledge_sessions(filter?, limit?)` and `get_knowledge_session_detail(session_id)` — served from the in-memory store when active and from disk (`<config_dir>/ai-sessions/`) otherwise.
- **Experimental AI block enrichment** (`#1389-b547`) — Opt-in Settings flag (`experimental_ai_block_enrichment`, default off). When enabled, every completed OSC 133 D block is enqueued to a bounded `mpsc` worker that asks the active AI Chat provider for a one-line `semantic_intent` and writes it back to the `CommandOutcome`. `CommandOutcome` gains a stable `id: u64` (dense per session) so the worker can target the exact record. Per-minute rate limit (~10/min), silent drop on full queue or disabled setting — never blocks the PTY path. Intent lines surface in the knowledge history overlay.
- **Drag & drop files onto folders in File Browser** — Dropping OS files/folders on a directory row in the File Browser moves them into that directory (hold `⌥`/`Alt` on macOS, `Ctrl` elsewhere, to copy instead). Name conflicts are silently skipped. Recursive transfers of directories prompt for confirmation. Enabled by flipping `dragDropEnabled` to `true` and routing Tauri's `onDragDropEvent` payload — which carries absolute OS paths — through a hit-tester. Terminal drops now paste shell-quoted absolute paths (previously only the basename was available via the browser `File.path` non-standard field, which broke commands referencing the dropped file).
- **AI Chat panel** — Conversational AI companion that sees the terminal as the user sees it. Side panel + settings tab (`Cmd+Alt+A` toggle, action `toggle-ai-chat`). Streaming markdown with syntax-highlighted code blocks and "Run this" action back to the active PTY. Multi-provider: Ollama (local, auto-detected on `localhost:11434`), Anthropic Claude, OpenAI, OpenRouter, custom base URL. Per-turn context assembly pulls `VtLogBuffer` clean text (configurable line budget), `SessionState`, recent `ParsedEvent`s, git branch/diff. API keys stored in OS keyring (service `tuicommander-ai-chat`). Conversations persisted to disk with load/list/delete. Terminal context menu actions (send selection / send error) and global hotkey. Settings at `Settings > AI Chat` (provider, model, base URL, temperature, context lines).
- **AI Agent loop (ReAct, Levels 2 + 3)** — Autonomous agent that observes a terminal and acts through six terminal tools: `read_screen`, `send_input`, `send_key`, `wait_for`, `get_state`, `get_context`, plus six Level 3 filesystem tools: `read_file`, `write_file`, `edit_file`, `list_files`, `search_files`, `run_command`. Filesystem access confined by `FileSandbox` path jail scoped to the repo root. `run_command` executes in a sandboxed subprocess with timeout. File-write safety checker blocks writes to sensitive paths (`.env`, credentials, CI configs). Pause/resume between iterations, user approval card for destructive commands (detected by `SafetyChecker`: `rm -rf`, `git reset --hard`, `DROP TABLE`, force push, `dd`, …). Structured command parser extracts executable + args for precise safety classification. Tool-call cards in `AIChatPanel` with collapsible output. Conversation schema v2 persists tool-call records alongside messages. `tokio-tracing` observability spans across the entire agent module.
- **Session knowledge store** — Per-session accumulator: command outcomes with exit code, duration, CWD, classification (`Success` / `Error{error_type}` / `TuiLaunched{app_name}` / `Timeout` / `UserCancelled` / `Inferred`), auto-correlated error→fix pairs, CWD history, TUI apps seen, terminal mode. Fed by OSC 133 semantic prompt markers (`OSC 133;A/B/C/D`) with an inferred-outcome fallback from the silence timer when OSC 133 is absent. Injected into the agent system prompt as a compact markdown summary. Surfaced in the UI as a collapsible `SessionKnowledgeBar` under the chat panel. Persisted to `<config_dir>/agent-knowledge/<session_id>.json` with a 2 s debounced background flusher. Tauri command: `get_session_knowledge`.
- **TUI app detection** — `tui_detect.rs` heuristics track alternate-screen enter/leave (`ESC[?1049h`/`l`) to classify `TerminalMode` as `Shell` or `FullscreenTui { app_hint, depth }`. Known signatures (vim, htop, lazygit, less, tmux, claude, …) populate `tui_apps_seen`. Agent tool set adapts: in TUI mode `send_input` prefers `send_key` semantics and `wait_for` polls the rendered screen rather than lines-since.
- **`ai_terminal_*` MCP tools (external agent surface)** — Six tools exposed to external MCP clients (Claude Code, Cursor, …) so a remote agent can observe and drive a TUICommander terminal: `ai_terminal_read_screen`, `ai_terminal_send_input`, `ai_terminal_send_key`, `ai_terminal_wait_for`, `ai_terminal_get_state`, `ai_terminal_get_context`. `send_input`/`send_key` always prompt the user and are rejected while the internal agent loop is active on the target session. Output passes through secret redaction.
- **ChoicePrompt parser variant** — New `ParsedEvent::ChoicePrompt { title, options, dismiss_key, amend_key }` detects Claude-Code-style numbered confirmation menus (footer matches `Esc to cancel · Tab to amend`; options match `[❯›>] <digit>[.)] <label>`; minimum two options; title heuristics require `?` or a verb prefix like "do you want"/"proceed"/"confirm"). Destructive labels (`no`, `cancel`, `reject`, `abort`, `deny`, `don't`/`do not`) flagged for styling. Piped into `SessionState.choice_prompt`, dispatched to plugins via `pluginRegistry.dispatchStructuredEvent("choice-prompt", …)`, and rendered as a PWA overlay. Desktop listener plays a warning sound when the prompt arrives on an inactive tab.
- **MCP upstream OAuth 2.1** — Full RFC 9728 (Protected Resource Metadata) + RFC 8414 (Authorization Server Discovery) flow for upstream MCP servers. `TokenManager` handles PKCE S256, code exchange, and refresh with a per-upstream semaphore that defeats thundering-herd refresh. `OAuthFlowManager` drives the PKCE dance, pending-flow state machine, and a localhost dev callback server. Native deep link `tuic://oauth-callback` completes the flow in the desktop app. `UpstreamAuth::OAuth2 { client_id, scopes, authorization_endpoint?, token_endpoint? }` joins `Bearer` as a credential type. `UpstreamError::NeedsOAuth { www_authenticate }` is surfaced on 401 so the registry transitions the upstream to `needs_auth` and the UI shows an "Authorize" button (AS origin displayed in the consent dialog to defend against AS mix-up). Tokens are persisted to the OS keyring as structured JSON (`OAuthTokenSet` with `expires_at`). Auto-triggered OAuth is gated behind explicit user consent.
- **OAuth 2.1 auth selector (Services settings)** — Per-upstream auth picker (`none` / `bearer` / `oauth2`). OAuth fields: client ID, scopes, optional authorization + token endpoints (defaults come from discovery). Live status includes `authenticating` ("Awaiting authorization…"). "Authorize" and "Cancel" buttons wired to the three OAuth commands.
- **Open with Default App (File Browser)** — Generic "Open with Default App" context-menu entry on any filesystem item defers to the OS handler. Complements the existing code-editor / markdown / HTML routing for known extensions.
- **Open File / Folder / Path + expanded Tools menu** — New app menu entries for opening arbitrary files, folders, and paths from disk, plus reorganised Tools submenu.
- **Plugin iframe reload** — Right-click context menu entry and `Cmd/Ctrl+R` reload for plugin iframes.
- **Smart Prompts Shell Script mode** — New "Shell script (direct run)" execution mode runs prompt content directly as a shell command without any AI agent. Executes via system shell (`sh`/`cmd`) in the repo directory with 60s timeout. Supports all context variables (`{branch}`, `{repo_path}`, etc.) and output targets (clipboard, toast, panel). Ideal for automating CLI pipelines like branch cleanup, linting, or metrics collection
- **TUIC SDK v1.0 expansion** — Plugin iframes now have access to the full SDK: `tuic.activeRepo()`, `tuic.onRepoChange()`, `tuic.getFile()`, `tuic.toast()`, `tuic.clipboard()`, `tuic.send()`/`tuic.onMessage()`, `tuic.theme`/`tuic.onThemeChange()`. Relative paths resolve against the active repo with traversal guard. SDK is auto-injected into same-origin URL-mode iframes. Interactive test page at `docs/examples/sdk-test.html`
- **Visual toast notifications** — `tuic.toast()` (SDK) and MCP `ui action=toast` now display visual toast notifications in the bottom-right corner with auto-dismiss (4s) and click-to-dismiss. Optional `sound` parameter plays a synthesized notification tone per level (info = soft blip, warn = double beep, error = descending sweep)
- **Multi-format Preview tab** — Clickable file paths, drag & drop, and File Browser now open preview-capable files in a dedicated tab: HTML (sandboxed iframe), PDF, images (PNG/JPG/GIF/WebP/SVG/AVIF/ICO/BMP), video (MP4/WebM/OGG/MOV), audio (MP3/WAV/FLAC/AAC/M4A), and plain text/data (TXT/JSON/CSV/LOG/XML/YAML/TOML/INI/CFG/CONF). File routing via `classifyFile()` utility.
- **Focus mode** (`Cmd+Alt+Enter`) — Hides sidebar, tab bar, and all side panels to maximize active tab content. Toolbar and status bar remain visible. Session-only (not persisted).
- **PWA echo classification** — Smart PTY input line sync for the mobile PWA: `classifyEcho()` rejects stale prefix echoes, accepts superset echoes (tab completion) immediately, and holds unrelated text during a 300 ms grace window after writes settle. Eliminates duplicate/deleted characters under latency.

### Security
- **`write_external_file` restricted to home-dir allowlist** — Tauri command now refuses writes outside `$HOME` to eliminate drive-wide write via crafted paths.
- **`stat_path` TCC guard (macOS)** — Returns a deterministic permission error instead of silently failing when the path is in a TCC-protected directory without access.
- **Smart Prompts `env_clear` + allowlist** — Shell and headless execution paths now clear the environment and re-inject a small allowlist before spawning, preventing repo-controlled env vars from leaking into subprocesses.
- **Smart Prompts shell-quoting** — Repo-controlled variables are shell-quoted in shell mode; eliminates injection through `{branch}`/`{repo_path}` containing metacharacters.
- **Smart Prompts run-config args argv-safe** — Argv-form execution (no intermediate shell), metacharacters in args now literal.
- **MCP headless/api prompt routes localhost-only** — The `/headless/prompt` and `/api/prompt` routes refuse non-localhost peers.
- **Settings API key masking** — API key fields are masked by default with an eye toggle to reveal; prevents shoulder-surfing while editing.
- **Duplicate env-var key detection** — Settings validates that each env-var key is unique per run config before save.
- **MCP OAuth hardening** — Unified `redirect_uri` between registry and commands (`#1260`); share `TokenManager` across http-client refresh paths (`#1270`); accept `None expires_at` as valid (`#1269`); defend against Authorization Server mix-up and display AS origin in the consent dialog (`#1268`); dropped dead constant-time comparison and documented the desktop-deep-link threat model (`#1266`).
- **File-stem ID validation** — Conversation and knowledge file IDs are now validated against path traversal patterns (`..`, `/`, `\`), blocking crafted IDs from escaping their storage directory.
- **OSC 133 data sanitization + secret redaction** — OSC 133 prompt markers are stripped from tool event output. API keys and tokens in terminal content are redacted before being persisted or sent to the AI provider.

### Changed
- **Experimental feature flags** — Settings > General now has an "Experimental Features" section with a master toggle and sub-flag for AI Chat. Features gated behind `experimental_features_enabled` are hidden until the user explicitly opts in.

### Fixed
- **Ctrl-U injection shell-family aware** — Prefix is now selected based on the detected shell family (POSIX/Windows) rather than the host platform, preventing bogus clears when mixing PowerShell with a POSIX shell or WSL.
- **`suggest:` parser robustness** — Dewraps the `suggest:` keyword when split across a newline on narrow terminals; accepts the wrap tail; bounds the overlay pipe-heuristic. Avoids an unconditional `String` allocation in `dewrap_suggest_keyword` (perf).
- **Keybindings capture cancel / replace** — Global hotkey restored when capture is cancelled; conflict-replace now unbinds the previous action explicitly and compares canonical action names.
- **Branch switch resilience** — `handleBranchSelectInner` body wrapped in `try/finally` so UI always returns to a clean state on failure.
- **Per-PR diff loading state** — Loading state is now scoped per-PR instead of being shared across all expanded PRs.
- **Plugin panel tabs evicted on repo switch** — Non-pinned plugin-panel tabs no longer leak across repositories.
- **App init repo-revision bump** — `repo-changed` now bumps the revision synchronously, avoiding a race where panels rendered with stale state.
- **AI Chat listener leaks** — Disposed stale `onCleanup` listeners and removed direct DOM manipulation hazards in `AIChatPanel` that could leak event handlers across re-renders.
- **Agent session identity binding** — Agent loop now binds to a specific PTY session ID at start, preventing tool calls from writing to a different terminal if tabs are switched mid-run.
- **Agent filesystem I/O off async runtime** — File system operations in the agent module moved to `spawn_blocking` with `Acquire`/`Release` atomics, avoiding blocking the Tokio runtime on large reads/writes.
- **PWA real-time idle/busy indicator** — Mobile PWA now subscribes to SSE `shell-state` events for live busy/idle status updates instead of polling.
- **Clipboard copy race condition** — Fixed race where clipboard copy could grab the wrong selection source when multiple terminals competed for the selection.
- **Cmd+Alt+letter shortcut combos** — `Cmd+Alt+<letter>` hotkey combinations now register correctly on macOS; AI Chat toggle button repositioned next to the mic button.

## [1.0.5] - 2026-04-12

### Added
- **PWA bidirectional live sync** — CommandInput in browser mode now uses delta-based bidirectional sync with the PTY, including echo deduplication and a slash dropup menu for command suggestions

### Fixed
- **Ghost terminals from resurrected store entries** — Prevented phantom terminal sessions from appearing when store entries were resurrected after session close
- **MCP debug invoke_js schema** — `__TUIC__` global is now hinted in the invoke_js schema for MCP debug introspection
- **vt-log-total emitted as bare number** — PTY event now emits the total as a plain number instead of a JSON object wrapper
- **PWA agent_type unified to "claude"** — Consistent `agent_type` value across the entire codebase
- **PWA slash menu detection** — Improved slash menu detection accuracy and fixed tab close countdown in browser mode
- **Cache-keepalive counter reset** — Counter now resets on user-input events instead of arbitrary triggers, preventing phantom busy→idle loops

### Changed
- **GitHub OAuth app** — Moved to TUICommander organization

## [1.0.4] - 2026-04-11

### Added
- **GitHub Issues panel** — Unified GitHub panel now shows issues alongside PRs. Filter by Assigned (default), Created, Mentioned, All, or Disabled. Each issue displays labels with GitHub-matching colors, assignees, milestone, and comment count. Actions: open in browser, close/reopen, copy number. Filter setting persisted in config
- **Terminal fontSize inheritance** — New terminal tabs inherit font size from the active terminal instead of always using the global default
- **Interactive GFM checkboxes** — Task-list items (`- [ ]`, `- [x]`, `- [~]`) in the Markdown panel are now clickable. Clicking cycles through unchecked → checked → in-progress (indeterminate) → unchecked. Changes are written back to the source file on disk. Source-line mapping ensures correct checkbox identification even with fenced code blocks
- **Dynamic debug store registry** — All frontend stores self-register via `debugRegistry.ts`. MCP `invoke_js` can now call `__TUIC__.stores()` and `__TUIC__.store(name)` for runtime introspection without manual bridge additions

### Fixed
- **Issue filter hydration race** — `issueFilter` now reads from `settingsStore` as single source of truth, eliminating a race where `githubStore` could read the pre-hydrate default
- **Shared issue action error** — Error messages for issue close/reopen are now scoped per-issue instead of shared across all expanded issues
- **Smart prompt variable guard** — Unresolved variables are now always flagged even when `contextVariables` are provided
- **Diagnostics crash in Settings** — Replaced non-null assertions (`!`) with optional chaining (`?.`) in GitHubTab diagnostics section to prevent crash when auth state hasn't resolved
- **Agent transition events missed on direct switches** — Switching directly between agents (e.g. claude → codex) without first returning to idle now correctly emits `agent-stopped` for the previous agent and `agent-started` for the new one. Previously, plugins filtered on a single `agentType` (like cache-keepalive) kept leaking internal state across the switch
- **xterm scrollbar still hidden on untouched terminals** — The previous CSS-only `.fade` override missed terminals that had never been interacted with (xterm's `_hide(e)` early-returns before adding `.fade` on the first reveal). Replaced with a JS observer that sources overflow from the xterm buffer model and forces inline opacity, covering streaming agents in background tabs
- **Overlay rectangles stuck during scrollback** — Suggest/intent row overlays no longer pin to the viewport top when scrolling through scrollback; the observer now listens to `onScroll` in addition to `onRender`

## [1.0.3] - 2026-04-10

### Added
- **Native file drag & drop** — Files dropped onto the window now use Tauri's native `onDragDropEvent` API, providing absolute OS paths instead of bare filenames. Dropped files write to the active PTY (for running agents) or open in the appropriate tab. Browser mode falls back to HTML5 drag events
- **Remote tab auto-close countdown** — When a remote (MCP) session closes, the tab name shows a live countdown (e.g. "PTY: Session 2 (45s)") before auto-removing after 60 seconds

### Fixed
- **Remote session tabs not appearing** — Fixed `pendingLocal` guard that blocked all `session-created` events when any local terminal had a null `sessionId` (e.g. during PTY reconnect after page reload). Now uses `browserCreatedSessions` set for accurate local/remote distinction
- **Remote tabs invisible for non-repo paths** — Sessions spawned from directories outside any tracked repository (e.g. `/tmp`) now fall back to the currently active repo/branch instead of creating an invisible orphan tab
- **Phantom question notifications** — Question detection now inspects only the single last chat line above the prompt instead of scanning 15 lines deep. Prevents false notifications from the user's own prior `?`-ending input or stale agent content across turn boundaries
- **xterm scrollbar disappearing** — Override xterm v6's auto-fade scrollbar behavior so the vertical scrollbar stays visible whenever there is scrollback content, instead of hiding when idle

## [1.0.2] - 2026-04-08

### Added
- **Global Workspace (experimental)** — Cross-repo workspace mode (`Cmd+Shift+X`) that promotes terminals from any repo into a unified view. Auto-layout distributes promoted terminals across panes. Sidebar globe icon indicates promoted state. Deactivating restores per-repo layouts. Repo isolation preserved in TabBar and pane persistence
- **Multi-monitor (POC)** — Secondary window with pane-only layout for multi-display setups
- **TUIC SDK `edit` command** — `tuic://edit/path` deep link opens files for editing in the host app
- **MCP debug introspection** — Full MCP log coverage and `eval_js` action in the debug tool for runtime diagnostics

### Changed
- **Markdown panel shortcut** — Changed from `Cmd+M` to `Cmd+Shift+M` to avoid conflict with macOS system minimize. All docs, tests, and keybinding defaults updated
- **Settings IPC** — `structuredClone` for plain-JS store paths eliminates redundant settings IPC round-trips

### Fixed
- **Suggest bar reliability** — Rewrote `conceal_suggest` as a simple single-chunk stream filter (no cross-chunk buffering that caused scroll freezes). Handles `\n`-delimited and Ink `\r`-segment layouts. Also: deferred PTY creation via ResizeObserver, stale actions cleared on user input, number prefixes stripped from chip display
- **Terminal focus restoration** — Focus returns to the active terminal after closing any modal dialog (CommandPalette, SmartPromptsDropdown, PromptDialog, PromptDrawer, BranchSwitcher, RunCommandDialog, RenameBranchDialog, CreateWorktreeDialog) and after PaneTree tab switches
- **Repo-switch freeze** — Eliminated visibility thundering herd that caused UI freeze when switching repositories with many terminals
- **Idle detection** — Replaced chrome-tick guard with spinner keepalive; tuned agent idle thresholds to prevent indicator flicker while maintaining responsive idle transitions
- **Plugin panel CSP** — URL-mode plugin tabs use `src=` attribute instead of fetch-inject to avoid Content Security Policy blocking
- **Tab ordering** — Tabs now follow spatial pane layout order in split mode
- **Keepalive replay** — Shell-state replayed after agent detection completes, preventing stale state after keepalive reconnection
- **Plugin localhost URLs** — `fetch_tab_html` now allows localhost URLs for local development dashboards

## [1.0.1] - 2026-04-06

### Added
- **Open file / New file** — `Cmd+O` opens a file picker and routes the result through the standard extension-based dispatch (`.md`/`.mdx` → markdown tab, `.html`/`.htm` → HTML preview tab, other → code editor). `Cmd+N` prompts for name + location, creates the empty file, and opens it the same way. Both also available from the command palette under the "File" category.
- **`file://` URLs in terminal** — Clickable `file:///…` URLs in terminal output are now recognized alongside plain paths. The prefix is stripped and the path resolved via the existing `resolve_terminal_path` flow.
- **Idle branch icons** — Sidebar branch icons (star, branch, worktree) turn grey when the repo has no active terminals, making it easy to spot repos with running sessions at a glance.
- **TUIC SDK for plugin iframes** — Every plugin iframe now receives `window.tuic`, a lightweight API for host integration. `tuic.open(path, {pinned?})` opens markdown files, `tuic.terminal(repoPath)` opens terminals, and `<a href="tuic://open/...">` links are intercepted automatically. Paths validated against known repos.
- **CSS popover tooltips** — Native popover-based tooltips for MCP tool descriptions, replacing title attributes with styled, multi-line popover panels
- **URL-mode plugin panels** — Plugin panels can now load external URLs directly via the `url` parameter in the MCP `ui` tool, enabling embedded dashboards like Mission Control
- **PWA reconnection banner** — Mobile PWA shows a reconnect dialog when the server goes down, with auto-reconnect on server recovery
- **Service worker fetch interception** — PWA service worker intercepts fetch requests for offline splash page and push subscription persistence across updates

### Changed
- **Ideas panel shortcut** — Moved from `Cmd+N` to `Cmd+Alt+N` so `Cmd+N` can serve the universal "New file" convention. Users with an override in `keybindings.json` keep their existing binding.
- **PWA push gate** — Web Push delivery is now gated on desktop window focus instead of a PWA heartbeat. Notifications fire whenever the desktop window is not focused (phone locked, screen off, etc.), finally matching the intended "wake the service worker" semantics. Removed the now-unused `POST /api/push/heartbeat` endpoint.
- **MCP tool consolidation** — 12 native MCP tools consolidated to 8 with unified dispatch routing, config defaults, and tmux-optimized meta-tool descriptions. Reduces tool-selection overhead for AI agents.
- **Pane layout persistence** — Split pane layouts now survive app restarts and branch switches, with terminal ID remapping on lazy restore

### Fixed
- **Duplicate history on WebSocket catch-up** — The raw PTY WebSocket handler racily registered its live subscription against the ring-buffer snapshot read, causing bytes written during the small gap to appear in both the catch-up replay and the live stream. Serialized `ring.write` + `ws_clients` broadcast under the same lock on both the producer and consumer sides, eliminating the duplication window.
- **Agent settings checkboxes** — Custom checkbox styles for agent settings toggles now render correctly across all themes
- **Flaky race condition test** — `ws_catchup_no_duplicate_with_concurrent_writer` test stabilized with explicit subscriber-attached synchronization barrier

## [1.0.0] - 2026-04-04

### Added
- **HTTP compression** — Gzip and Brotli compression for all HTTP responses >860 bytes via tower-http `CompressionLayer`. Auto-negotiated via `Accept-Encoding`
- **PWA lazy loading** — Mobile terminal view loads only the last 100 lines initially, then lazy-loads older output on scroll-up with viewport anchoring
- **MCP Debug Tool** — Dev-only `debug` MCP tool with `agent_detection`, `logs`, and `sessions` actions for diagnosing PTY and agent detection issues
- **Smart Prompt Variables** — 12 new context variables: `remote_url`, `current_user`, `repo_owner`, `repo_slug`, `dirty_files_count`, `branch_status`, `pr_author`, `pr_labels`, `pr_additions`, `pr_deletions` (31 total)
- **Variable Insertion Dropdown** — Smart prompt editor includes a dropdown below the content textarea with all available variables grouped by Git/GitHub/Terminal, with descriptions; click inserts `{variable}` at cursor position
- **Prompt Editor Tags** — Prompt rows in Cmd+K drawer show inline badges for execution mode (inject/headless), built-in status, and placement tags
- **Dev Debug Console** — `window.__debug` exposed in dev mode with all SolidJS stores + Tauri `invoke`/`listen` for browser console debugging
- **File Browser Tree View** — Toggle between flat list and tree view in the file browser panel. Tree view shows a collapsible hierarchy with lazy-loaded subdirectories on first expand
- **Diff Scroll View** — All-files continuous scroll view showing every changed file (staged + unstaged) with collapsible sections, per-file stats, and clickable filenames. Toggle via toolbar or `Cmd+Shift+G`
- **Command Palette File Search** — Type `!` in the command palette (`Cmd+P`) to search files by name, `?` to search file contents with highlighted matches. Results open in editor tabs
- **Update Progress Dialog** — Modal progress bar during update downloads with percentage and status text
- **Cross-Terminal Search** — Type `~` in the command palette to search text across all open terminal buffers. Results show terminal name, line number, and highlighted match. Selecting a result navigates to the terminal and scrolls to the matched line
- **Search Mode Commands** — Explicit "Search Terminals", "Search Files", and "Search in File Contents" commands in the palette make prefix modes discoverable
- **Global Hotkey Validation** — Frontend validates key combos before sending to Tauri, showing clear error messages for unsupported keys (e.g. `<` on ISO keyboards) instead of cryptic parser errors
- **Global Hotkey** — Configurable OS-level shortcut to toggle window visibility from any app. Set in Settings > Keyboard Shortcuts. Toggle cycles: hidden → show+focus, unfocused → focus, focused → hide. Uses `tauri-plugin-global-shortcut`; hidden in browser/PWA mode
- **Unified Repo Watcher** — Single recursive watcher per repository with per-category debounce (Git/WorkTree/Config), replacing separate HEAD and index watchers. Uses `notify-debouncer-full` with `.gitignore`-aware filtering
- **Gitignore Hot-Reload** — Editing `.gitignore` rebuilds the watcher's ignore filter without restart
- **File Icon Provider** — New `ui:file-icons` plugin capability; `tuic-vscode-icons` plugin provides VS Code-style file icons in the file browser tree view
- **Plan Auto-Open** — Restores active plan from `.claude/active-plan.json` on startup; `plans/` directory watcher detects new plan files created externally
- **macOS TCC Access Dialog** — Shows a guided dialog when macOS denies access to a repository directory, pointing the user to Full Disk Access settings

### Changed
- **Structured event tokens** — New plain-prefix format (`intent:`, `action:`, `suggest:`) replaces bracket syntax (`[[intent:...]]`). Both formats supported for backward compatibility. Plain-prefix parsing is agent-gated to prevent false positives from CLI output
- **MCP system prompt** — Agents now receive `action:` token instruction alongside `intent:` and `suggest:`, all documenting column-0 requirement

### Fixed
- **Terminal Scroll Lock** — Fixed viewport jumping away from scroll position when output arrives while scrolled up. Root cause: xterm's auto-scroll during writes falsely disengaged the ViewportLock
- **Terminal Close Confirmation** — Shells in startup (.zshrc/.zprofile loading) no longer trigger close confirmation; only shells that have completed initialization and are running a user process (agents, htop, npm) prompt for confirmation
- **File Browser Repo Switch** — File browser now updates instantly when switching between repositories via sidebar; previously showed stale files from the old repo due to unbatched reactive updates and an async race condition
- **Clipboard Paste** — Restored `tauri-plugin-clipboard-manager` so Cmd+V paste works in terminals (WebView requires explicit capability for `navigator.clipboard.readText()`)
- **Agent Detection** — Claude Code installs its binary as a version number (`~/.local/share/claude/versions/2.1.87`); `process_name_from_pid` now scans parent directory names when the basename doesn't match a known agent
- **HMR Session Loss** — Vite HMR reloads no longer close PTY sessions; `beforeunload` in Tauri mode skips session cleanup so `list_active_sessions` can re-adopt surviving sessions
- **Git Panel Label** — "Changes" section renamed to "Changes (unstaged)" for clarity
- **Diff Tab Focus** — Opening a diff tab now deactivates terminal, markdown, and editor tabs to prevent keyboard conflicts
- **Plan File Events** — Plan-file events with absolute paths now recognized regardless of CWD
- **iPad Touch Scroll** — Terminal output view now scrolls with touch gestures on iPad; `touch-action: pan-y` overrides the global `manipulation` that blocked iOS pan recognition
- **Sidebar Double-Tap** — Repo and branch selection on iOS no longer requires two taps; `:hover` rules that caused iOS sticky hover wrapped in `@media (hover: hover)`
- **Tab Double-Tap** — Same iOS sticky hover fix applied to tab bar (tab highlight, close button reveal, specialized tab types)
- **Shell State Idle** — Status line ticks from Claude Code no longer block idle detection; idle fires after 3s of real output silence even with status line ticking
- **Usage Exhausted** — New `ParsedEvent::UsageExhausted` detects "out of extra usage" messages with optional reset time
- **Agent Detection Speed** — Event-driven detection on shell-state transitions (immediate on idle, 500ms debounce on busy) replaces 3s polling; ~30x fewer syscalls
- **Agent Tracking Leak** — Module-level `discoveryAttempted` and `nullStreak` maps cleaned up when terminal is removed
- **VtLogBuffer Cursor** — `total_lines()` is now a monotonic counter that doesn't decrease on eviction, fixing paginated reads for mobile/REST clients

### Changed
- **Watcher Backend** — Upgraded from `notify-debouncer-mini` to `notify-debouncer-full`; deleted legacy `head_watcher` module
- **Smart Prompts Management** — Settings tab removed; all management consolidated in the Cmd+K drawer (edit, enable/disable, create, delete)
- **Prompt Drawer UI** — Compact font sizing aligned with command palette conventions; editor dialog layout improved with side-by-side execution mode + auto-execute fields
- **Tailscale HTTPS** — Auto-detects Tailscale daemon, provisions TLS certificates via Local API, serves HTTP+HTTPS on same port (dual-protocol). QR code uses `https://` with Tailscale FQDN when TLS active. Background cert renewal every 24h. Cross-platform (macOS, Linux, Windows)
- **PWA Push Notifications** — Web Push from TUICommander directly to mobile PWA clients. VAPID key generation, push subscription management via `/api/push/*` endpoints, service worker with push/notificationclick handlers. Rate limited (1 per session per 30s). iOS standalone detection with guidance
- **Smart Prompts** — AI automation layer with 24 built-in context-aware prompts
  - Toolbar dropdown (Cmd+Shift+K) with category grouping and search
  - SmartButtonStrip in Git Panel Changes tab and PR Detail Popover
  - Command Palette integration (all prompts with "Smart:" prefix)
  - Branch context menu integration
  - Inject mode: PTY write into active agent with idle check
  - Headless mode: one-shot agent execution with configurable per-agent templates
  - Auto-resolved context variables ({diff}, {branch}, {pr_number}, etc.)
  - Settings > Smart Prompts tab for management (enable/disable, edit, reset to default)
  - Settings > Agents: headless command template per agent
- **MCP Per-Repo Scoping** — Each repo can define which upstream MCP servers are relevant via an allowlist in repo settings (3-layer: per-repo > `.tuic.json` > defaults). Quick toggle via **Cmd+Shift+M** popup with live status, transport badges, tool counts, and per-repo checkboxes
- **Side-by-Side Diff Viewer** — Split and unified view modes with `@git-diff-view/solid`, word-level highlighting, and synchronized scrolling. Toggle persisted in ui-prefs.
- **Hunk & Line-Level Restore** — Revert individual hunks or selected lines in working tree and staged diffs via `git apply --reverse`. Click lines to select, shift-click for ranges, floating action bar with line count.

- **Smart Prompts API Mode** — New "API (LLM direct)" execution mode calls LLM providers directly via HTTP API (genai crate), no terminal or agent CLI needed. Global provider/model/API key config in Settings > Agents. Per-prompt system prompt. Supports OpenAI, Anthropic, Gemini, OpenRouter, Ollama, and any OpenAI-compatible endpoint. API key stored in OS keyring
- **Notification Bell Enhancements** — CI recovery ("CI Passed") notifications, background git operation results, worktree creation events. Empty state shows "No notifications" instead of 1px dropdown.
- **TCP Port Retry** — MCP HTTP server tries up to 3 adjacent ports when the configured port is busy, with clear error message on failure.
- **Base Branch Tracking** — Branches store a base ref in git config (`tuicommander-base`), showing ahead/behind relative to base in sidebar. "Update from base (rebase)" in context menu. Inline branch create form includes a base ref selector with grouped Local/Remote refs. Auto-fetches remote refs before creation
- **Edit File Button** — Diff tab toolbar includes "Edit file" button to open the file in the default editor
- **OSC 8 File Links** — Terminal `file://` URIs from OSC 8 hyperlinks now open in the system file opener
- **Tailscale Recheck** — Settings > Services tab includes a "Recheck" button for Tailscale HTTPS status
- **Profiling Infrastructure** — Scripts in `scripts/perf/` for IPC latency, PTY throughput, CPU recording, Tokio console, and memory snapshots. See `docs/guides/profiling.md`

### Changed
- **PTY write coalescing** — Terminal writes are accumulated per animation frame via `requestAnimationFrame` (~60/sec) instead of calling `terminal.write()` for every PTY event (hundreds/sec during burst output). Reduces xterm.js render passes and WebGL texture uploads
- **Async git commands** — All ~25 git commands converted to async with `tokio::task::spawn_blocking`, preventing git subprocess calls from blocking Tokio worker threads. `get_changed_files` merged from 2 sequential subprocesses to 1
- **Watcher-driven git cache** — `repo_watcher` invalidates git caches immediately on file system changes instead of relying on 5s TTL. TTL raised to 60s as safety net for missed watcher events. Most IPC calls hit cache (~0.2ms) instead of spawning git (~20-30ms)
- **Process name via syscall** — `proc_pidpath` (macOS) / `/proc/pid/comm` (Linux) replaces `ps` fork for terminal process detection. Eliminates ~100 fork+exec/min with 5 terminals open
- **MCP RwLock** — MCP upstream `HttpMcpClient` uses `RwLock` instead of `Mutex`. Tool calls use read lock (concurrent); only reconnect takes write lock
- **Double serialization eliminated** — PTY parsed events serialized once with `serde_json::to_value`, reused for both Tauri IPC emit and event bus broadcast
- **PTY read buffer** — Increased from 4KB to 64KB for natural batching of burst output, reducing IPC events during high-throughput agent output
- **Bundle splitting** — Vite `manualChunks` splits xterm, codemirror, diff-view, markdown into separate chunks. Lazy-load SettingsPanel, ActivityDashboard, HelpPanel with `lazy()` + `Suspense`
- **Conditional StatusBar timer** — 1s timer only runs when merged PR or rate limit is active, eliminating ~60 signal writes/min during normal operation
- **ActivityDashboard reactivity** — Removed `{ equals: false }` from snapshot signal; SolidJS default equality check prevents unnecessary `<For>` diffs every 10s
- **SmartButtonStrip** — Extracted as reusable split button component with dropdown, spinner, click-outside, error callbacks, last-used memory. Integrated in git-changes, git-branches, pr-popover

### Fixed
- **Browser mode parsed events** — Structured events (suggest, status-line, rate-limit, question, progress, etc.) now work in browser/remote mode via WebSocket, not just Tauri desktop.
- **Stale suggestion chips** — Follow-up suggestions no longer reappear from buffer re-scans during resize/tab-switch; requires agent idle state.
- **Git spawn error diagnostics** — "Spawn failed" errors now include the working directory path in the log message for easier debugging.
- **MCP upstream URL overflow** — Long URLs in the Services tab no longer push action buttons off-screen; URLs now truncate with ellipsis
- **Stale worktree pruning** — `get_worktree_paths` now runs `git worktree prune` before listing and skips entries whose directory no longer exists on disk
- **Commit textarea auto-expand** — Textarea grows with content using `scrollHeight`, switches to scrollable when exceeding max-height
- **Agent polling race** — Fixed `useAgentPolling` early return that prevented interval creation when sessionId was set after terminal add. Added 3-poll debounce before clearing agent status
- **False idle from silence timer** — Chrome-chunk arrivals no longer reset the silence timer, preventing false idle transitions during streaming output
- **Tailscale cert fallback** — Falls back to CLI cert provisioning on macOS App Store builds where Local API is unavailable
- **rustls CryptoProvider** — Explicitly install `ring` CryptoProvider at startup to prevent "no process-level CryptoProvider" panic

## [0.9.9] - 2026-04-02

### Added
- **Copy on select** — Auto-copy terminal selection to clipboard (enabled by default in Settings > Appearance)
- **Terminal bell styles** — Configurable bell: none, visual (screen flash), sound (via notification system), or both
- **Scroll shortcuts** — Cmd+Home (top), Cmd+End (bottom), Shift+PageUp, Shift+PageDown
- **Zoom pane** — Cmd+Shift+Enter maximizes/restores the active split pane
- **Ctrl+Tab / Ctrl+Shift+Tab** — Native tab switching via macOS NSEvent monitor (bypasses WKWebView interception)
- **Dictation auto-send** — Option to automatically press Enter after transcription completes
- **Environment flags UI** — Per-agent environment variable injection from Settings > Agents
- **--bare flag** — CLI option for minimal startup
- **ANSI anomaly logging** — Diagnostic logging for unusual terminal escape sequences (scroll-jump investigation)

### Changed
- **Watcher v3** — Replaced `notify-debouncer-full` with raw `RecommendedWatcher` and manual per-category debounce
- **PTY rendering** — Replaced DiffRenderer with cursor-up clamping for simpler escape sequence handling
- **Bell implementation** — Moved from xterm.js built-in `bellStyle` option to manual `onBell` handler with notification system integration
- **Prompt library shortcut** — Menu accelerator corrected from Cmd+K to Cmd+Shift+K (Cmd+K is clear scrollback)
- **Git panel shortcut** — Menu accelerator corrected from Cmd+Shift+G to Cmd+Shift+D (Cmd+Shift+G is diff scroll)
- **Tab switching** — Removed Cmd+Shift+[/] defaults (unreliable on non-US keyboards), Ctrl+Tab is now primary

### Fixed
- **Rate limit warning** stuck in status bar after expiry
- **Terminal CWD** falls back to active repo path when PTY reports no working directory
- **Agent events** — Plugin system now emits `agent-started` / `agent-stopped` events correctly
- **Copy-on-select** feedback — Shows "Copied to clipboard" in status bar
- **Keyboard shortcuts help** — Added 12 missing shortcuts to the help panel
- **Documentation** — Corrected Cmd+K → Cmd+Shift+K references across 6 doc files, updated tab switching docs

## [0.9.7] - 2026-03-26

### Added
- **Sidebar Plugin Panels** — New `ui:sidebar` capability lets plugins register collapsible panel sections in the sidebar below the branch list. Panels display structured data (items with icon, label, subtitle, badge, context menu) scoped per-repo. Built-in plan plugin migrated from Activity Center to sidebar panel
- **Multi-target context menu actions** — Plugins can now register actions in branch, repo, and tab context menus (not just terminal). New `registerContextMenuAction()` API with target types and typed context
- **Open in GitHub** — Branch and repo right-click context menus now include "Open in GitHub" (opens branch/repo on github.com) and "Open PR" (direct link to the PR if one exists)
- **Startup notification suppression** — PTY sessions now suppress Question, RateLimit, and ApiError notifications during the initial output burst (e.g. `claude --continue` replaying conversation history). Grace ends after 5s without output or 120s max

### Fixed
- **Plugin double-dispose crash** — Plugin disposables are now idempotent; calling `dispose()` twice no longer crashes with "undefined is not an object (evaluating 'listeners[eventId].handlerId')"
- **awaitingInput not cleared on idle→busy** — Question notifications are now properly cleared when the agent resumes work (idle→busy transition). The null→busy case is excluded since the agent hasn't been idle yet

## [0.9.6] - 2026-03-25

### Added
- **Inter-Agent Messaging** — New `messaging` MCP tool for agent-to-agent coordination. Agents register with their `$TUIC_SESSION` identity, discover peers via `list_peers`, and exchange messages via `send`/`inbox`. Dual delivery: real-time push via MCP channel notifications (SSE) when `--dangerously-load-development-channels` is active, plus polling fallback via inbox. Spawned Claude Code agents automatically get the channels flag. TUICommander acts as the messaging hub — no external daemon needed
- **Multi-instance socket coexistence** — Multiple TUICommander instances (e.g. release + dev build) now coexist safely. First instance binds `mcp.sock`, subsequent instances fall back to `mcp-{pid}.sock`. Bridge auto-discovers live sockets with `TUIC_SOCKET` env override. Stale sockets cleaned on startup
- **Enriched health endpoint** — `/health` now returns `uptime_secs`, `session_count`, and `socket_path` for monitoring
- **Session close reasons** — `session-closed` events include a `reason` field (`process_exit`, `explicit_close`) for debugging session lifecycle

### Changed
- **Terminal scroll tracking** — Consolidated 20 iteratively-patched scroll fixes into a self-contained `ScrollTracker` class with 26 unit tests. Replaces inline `trackedScrollState`, `lastKnownVisible`, and `updateTrackedScroll` with a testable state machine that handles visibility inference, alternate buffer guards, and re-entrancy suppression

### Fixed
- **Terminal scroll lock** — New write-based `ViewportLock` keeps the viewport anchored when user scrolls up to read. Programmatic scrolls (from agent output) are intercepted during `terminal.write()` and restored via xterm's `scrollToLine()` API. Zero overhead when at bottom
- **Session tab visibility** — MCP-created sessions now match to repos using ancestor path matching (subdirectory of repo root or worktree), fixing a race condition with branch stats loading
- **Question detection** — Removed `q.starts_with(t)` prefix match that could produce false positive ghost notifications on short screen rows
- **PTY creation consolidation** — Shell PTY creation in MCP transport now delegates to `spawn_pty_session`, fixing a missing `last_output_ms` insertion for REST-created sessions

## [0.9.5] - 2026-03-23

### Added
- **GitHub OAuth Login** — New "GitHub" tab in Settings with one-click Device Flow authentication. Stores token securely in OS keyring (macOS Keychain, Windows Credential Manager, Linux Secret Service). Eliminates manual PAT management and missing-scope issues. Token resolution priority: env vars → OAuth keyring → gh CLI
- **Branch Panel** — New Branches tab (4th tab) in the Git Panel with full branch management: checkout (with dirty-worktree stash/force/cancel dialog), create, delete (safe + force), rename, merge, rebase, push (auto-sets upstream), pull, fetch, inline search, context menu, stale dimming (>30 days), merged badge, ahead/behind counts, prefix folding, and recent branches from reflog. `Cmd+G` opens the Git Panel directly on the Branches tab; clicking the sidebar "GIT" vertical label also lands on Branches
- **Worktree Agent Bridge** — MCP `worktree action=create` now returns a `cc_agent_hint` field for Claude Code clients, guiding CC to spawn a subagent that works in the worktree using absolute paths. Works around CC's inability to change working directory mid-session
- **Auto-retry on API errors** — Terminal sessions automatically retry when the AI provider returns server errors (5xx, rate limits). Configurable per-agent in Settings
- **Plans Panel** — Scans `plans/` directory to populate the PlanPanel with project plans
- **HTML Preview** — New panel for previewing HTML files with "Open in Browser" action
- **Cmd+Q confirmation** — Shows a confirmation dialog when quitting with active terminal sessions

### Fixed
- **Terminal scrollbar jank** — Eliminated a redundant native scrollbar on the xterm viewport that was updating out of sync with xterm v6's custom scrollbar widget, causing a visible thumb-resize flash on each write
- **Terminal scroll stability** — Seven distinct root causes for viewport-jump-to-line-0 identified and fixed: escape-sequence jumps, buffer contraction drift, baseY staleness on idle sessions, alternate buffer corruption, hidden terminal viewportY drift, hidden→visible transition guards, and WebGL atlas rebuild timing
- **Suggest overlay** — Added close button (X) and anchored the suggest token regex to start-of-line to prevent false matches
- **File path linking** — Terminal file paths followed by sentence punctuation (`.`, `,`, `)`) are now correctly clickable
- **GitHub settings** — "Connect to GitHub" button now appears after disconnect or fetch failure
- **Config test initializer** — Added missing `auto_retry_on_error` field

### Removed
- **Lazygit integration** — Replaced by the native Branch Panel. `Cmd+G` is reassigned to the Branches tab

### Security
- Updated `tar` crate 0.4.44 → 0.4.45 to fix RUSTSEC-2026-0067/0068

## [0.9.4] - 2026-03-19

### Added
- **Cross-repo knowledge base** — New `knowledge` MCP tool powered by mdkb. Actions: `setup` (auto-provisions mdkb upstream per repo), `search` (hybrid BM25+semantic fan-out across repo groups), `code_graph` (cross-repo call graph queries), `status` (indexing status). Provisioned upstreams persist in `mcp-upstreams.json`
- **Stdio upstream `cwd` field** — MCP upstream servers using stdio transport can now specify a working directory. Required for mdkb which uses cwd as project root
- **Boot-time upstream auto-connect** — Saved MCP upstream servers in `mcp-upstreams.json` now connect automatically on app launch (previously required UI interaction)
- **CI Auto-Heal** — When CI checks fail on a branch with auto-heal enabled and an active agent terminal, TUICommander fetches the failure logs via `gh run view --log-failed`, waits for the agent to be idle, and injects the logs with a fix prompt. Up to 3 attempts per cycle. Toggle per-branch in the PR detail popover
- **PWA WebSocket auto-reconnect** — When the browser closes the WebSocket (e.g. mobile backgrounding), the terminal now auto-reconnects with exponential backoff (1s→30s, up to 10 attempts). A pulsing "Reconnecting" banner shows progress. PTY sessions survive on the server — no data loss
- **Chunked backlog streaming** — Initial PTY catch-up on WebSocket connect is now sent in 64KB chunks instead of one giant frame. Supports `?offset=N` for delta-only catch-up on reconnect, skipping already-received data

### Fixed
- **MCP stale session auto-recovery** — When a `tools/call` or SSE request arrives with a session ID the server no longer recognizes (e.g. after app restart), the session is re-registered automatically instead of returning a `-32600` error. Only requests missing the header entirely are rejected

### Changed
- **MCP tool rationalization** — Removed `git` tool (thin CLI wrappers CC does natively). Replaced with `github` tool (`prs` for batched PR+CI, `status` for cross-repo aggregate) and `worktree` tool (`list`, `create` with optional `spawn_session`, `remove`). Tool count 7→8, action count 24→21
- **Session output now includes exit status** — `session action=output` returns `exited` (bool) and `exit_code` (number|null) so agents know when a teammate has finished
- **Workspace list includes ahead/behind** — `workspace action=list` now returns `ahead`/`behind` counts for repos with remotes, eliminating follow-up calls

## [0.9.3] - 2026-03-18

### Added
- **Dictation instant mode** — Long-press threshold slider now starts at 0 (was 200ms). When set to 0, any keypress activates dictation immediately without short-press pass-through. UI shows "Instant" label

### Fixed
- **Terminal scroll jump on long sessions** — After long idle periods, the terminal viewport would jump to line 0 on any resize event. Root cause: `trackedScrollState.baseY` drifted from reality because xterm's `onScroll` doesn't fire when `baseY` grows while the user is scrolled up. Now updated on every write callback
- **Worktree orphan cleanup** — When a linked worktree was deleted externally, its terminals remained live in the store, preventing the stale branch from being removed from the sidebar. Now closes orphaned terminals automatically
- **MCP server instructions for agent teams** — Restored server identity ("terminal session orchestrator") and explicit `session action=create` / `agent action=spawn` workflow steps that were removed in the v0.9.2 slim-down. Added Claude Code-specific hint for teammate PTY creation (conditional on clientInfo)
- **Post-merge cleanup branch switch** — `switch_branch` invoke used wrong parameter name (`branch` instead of `branchName`), causing the post-merge cleanup step to fail silently
- **Tauri invoke parameter mismatches** — Fixed 5 broken `invoke()` calls: `close_pty` used `id` instead of `sessionId` (RepoSection, PrDetailPopover), `write_pty` used `id` instead of `sessionId` (pluginRegistry), `write_plugin_data` and `read_plugin_data` used `plugin_id` instead of `pluginId`
- **Close PTY error resilience** — Terminal close loops in worktree cleanup, PrDetailPopover, and RepoSection now catch errors from already-dead PTY sessions instead of aborting the entire cleanup
- **Bridge version not bumped** — `make bump` now includes `src-tauri/crates/tuic-bridge/Cargo.toml`

## [0.9.2] - 2026-03-18

### Added
- **Dictation long-press hotkey** — Replaced tauri-plugin-global-shortcut with tauri-plugin-user-input for push-to-talk activation. Now supports 140+ keys (vs the limited set before) and long-press detection: short press passes through as normal input, holding the key beyond a configurable threshold (default 400ms) starts dictation. Key repeat is automatically filtered. Threshold is adjustable in Settings > Dictation (200–1000ms)
- **Cmd+F search in diff panels** — DiffTab now supports `Cmd+F` text search via SearchBar + DomSearchEngine, matching the markdown viewer search experience
- **Copy Path in viewer tab context menus** — Right-click on diff, markdown (file type), and editor tabs to copy the file path to clipboard
- **Click-to-diff in Git Panel** — Changes tab: clicking a file row opens its diff directly. Log tab: clicking a file in an expanded commit opens its diff at that commit hash

### Changed
- **Slimmer MCP server instructions** — Removed redundant tool table from MCP instructions (tool schemas already describe actions), switched from markdown tables to compact lists

### Fixed
- **File operations in worktrees** — Markdown viewer, file browser, code editor, and git panels now correctly resolve file paths against the worktree directory instead of the main repo root. Previously, opening a markdown file or browsing files while on a linked worktree branch would fail with "file not found" or show files from the wrong branch
- **MCP bridge socket path** — tuic-bridge was looking for the Unix socket in `tuicommander/` instead of `com.tuic.commander/`, preventing MCP connections from Claude Code and other agents
- **Text selection in diff panels** — DiffTab and PrDiffTab now allow text highlighting and copying via `user-select: text`
- **Submodule entries in working tree status** — Submodules no longer appear as regular files in the Changes tab
- **SearchBar placeholder encoding** — Fixed literal `\u2026` showing instead of ellipsis character in "Find…" placeholder
- **SearchBar counter text wrapping** — "No results" text no longer wraps to a second line
- **PR badge click on non-active repo** — Clicking a PR status badge (e.g. "Conflicts") on a branch belonging to a non-active repo now correctly opens the PR detail popover instead of silently doing nothing
- **PWA input duplication with agents** — Live PTY sync sent Ctrl-U bundled with text in a single PTY write. Cooked-mode shells (bash/zsh) handled this correctly, but raw-mode apps (Claude Code/Ink, Aider) don't process Ctrl-U when bundled with text in the same read — causing progressive input duplication. Live sync is now disabled for detected agent sessions

## [0.9.1] - 2026-03-16

### Added
- **File browser content search** (`Cmd+Shift+F`) — full-text search across file contents with case-sensitive, regex, and whole-word options. Results stream progressively and are grouped by file. Click any result to open the file at the matched line. Binary files and files >1 MB are automatically skipped
- **Color picker for group colors** — Visual color picker dialog with 8 preset swatches, native browser color input, and clear button. Shared `ColorSwatchPicker` component used in sidebar and settings tabs ([#9](https://github.com/sstraus/tuicommander/pull/9), thanks @antoniovizuete)

### Fixed
- **File drag & drop** — Drag & drop now works correctly in Tauri. Replaced broken HTML5 `File.path` (undefined in Tauri webviews — Electron-only API) with `getCurrentWebview().onDragDropEvent()` which provides real absolute paths. When a terminal has an active PTY session, dropped files are forwarded as paths to the terminal (enabling Claude Code image drops). Otherwise, `.md`/`.mdx` files open in the Markdown viewer and all others in the Code Editor. A global `dragover`/`drop` `preventDefault` prevents the browser-navigation white screen when dropping onto non-terminal panels
- **Terminal scroll jump to top** — Resizing the terminal while scrolled up with pending output data no longer jumps to line 0. Root cause: `doFit()` mixed a fresh `buf.baseY` (inflated by incoming writes) with a stale `trackedScrollState.viewportY`, producing an inflated `linesFromBottom` that went negative when `fitAddon.fit()` shrank `newBase`. Fixed by using `trackedScrollState.baseY` for both sides of the subtraction
- **MCP Unix socket robustness** — A stale socket file from a crashed previous run no longer blocks MCP tool loading. `SocketGuard` RAII struct removes the socket on `Drop` (crash-safe cleanup). Bind retries up to 3 times (×100 ms) removing any stale file before each attempt. `get_mcp_status` liveness check upgraded from `socket_path().exists()` to a real `UnixStream::connect()` probe — preventing the bridge from returning `tools: []` against a dead socket
- **File browser: content search mode toggle icon** — The `C`/`F` mode toggle button in the file browser search bar was invisible due to a missing CSS size rule on the SVG. Fixed with explicit `width: 14px; height: 14px` on `.modeToggle svg`
- **`.tuic.json` scripts exclusion (security)** — Scripts (`setup`, `run`, `archive`) are never merged from the repo-local `.tuic.json` file. Only worktree and workflow settings are team-overridable; scripts must be configured locally per-developer to prevent arbitrary code execution via a checked-in config file
- **Windows: CMD window flash** — Background process spawns (git, agent detection, plugin execution, `where` lookups) no longer flash visible console windows on Windows. Applied `CREATE_NO_WINDOW` flag to all background `Command::new` callsites. Interactive spawns (IDE/terminal launches) unaffected. MCP stdio server stderr now forwarded to tracing on Windows instead of being silently dropped ([#7](https://github.com/sstraus/tuicommander/issues/7))
- **CI: Windows clippy** — Sidecar stub now creates `.exe` variant on Windows, fixing Tauri build.rs resource resolution
- **Tests: 34 broken test expectations** aligned with current implementation (mock mismatches, timing, security-excluded `.tuic.json` scripts)

## [0.9.0] - 2026-03-14

### Added
- **Git Panel** — Tabbed side panel (`Cmd+Shift+D`) replacing the Git Operations Panel floating overlay and standalone Diff Panel. Three tabs: Changes (staging/unstaging, commit with amend, discard, glob filter, per-file diff counts), Log (virtual scroll + Canvas commit graph with lane assignment and Bezier connections), Stashes (apply/pop/drop). History and Blame are collapsible sub-panels within Changes (not separate tabs). Keyboard navigation: Escape to close, Ctrl/Cmd+1–3 to switch tabs
- **Canvas-based commit graph** — Visual commit graph in the Log tab rendered on Canvas with lane assignment, 8-color palette, ref badges, and Bezier curve connections between parent/child commits
- **Ideas panel image paste** — `Ctrl+V` / `Cmd+V` pastes clipboard images into notes. Images saved to disk, displayed as thumbnails, and sent as absolute paths when forwarding to terminal (so AI agents can read them). Supports PNG, JPEG, WebP, GIF up to 10 MB. Image-only notes (no text) are allowed. Cleanup on delete
- **Ideas panel in-place edit** — Edit now preserves note identity (no ID change). `Escape` cancels edit mode
- **Archive script** — Per-repo lifecycle hook that runs before a worktree is archived or deleted; non-zero exit blocks the operation. Configurable via Settings → Repository → Scripts or `.tuic.json`
- **Repo-local config (`.tuic.json`)** — Team-shareable configuration file in the repository root. Three-tier precedence: `.tuic.json` > per-repo app settings > global defaults. Covers base branch, scripts, worktree storage, merge strategy, and more
- **PR Review button** — Review button in the PR Detail Popover spawns a terminal running the agent's "review" run config with interpolated PR variables (`{pr_number}`, `{branch}`, `{base_branch}`, `{repo}`, `{pr_url}`). Shown only when an active agent has a run config named "review"

### Changed
- **DiffPanel removed** — Standalone Diff Panel replaced by the Git Panel's Changes tab. `Cmd+Shift+D` now opens the Git Panel
- **Git Operations Panel removed** — Floating overlay replaced entirely by the docked Git Panel
- **Git Panel: History/Blame as sub-panels** — History and Blame moved from separate tabs to collapsible sub-panels within the Changes tab, reducing tab count from 5 to 3
- **Updater: beta channel removed** — Only stable and nightly update channels remain
- **Resume banner UX** — Now accepts Space or Enter to resume agent session; other keys dismiss the banner
- **Shell state derivation** — Moved shellState (busy/idle) from frontend timer-based derivation to Rust-authoritative AtomicU8 CAS transitions. Frontend syncs on remount via `get_shell_state` Tauri command
- **tuic-bridge standalone crate** — Extracted from main binary into an independent workspace crate for cleaner builds

### Fixed
- **Shell state false oscillation** — Mode-line ticks no longer cause false busy/idle transitions; question notifications fire correctly; completion sound suppressed when terminal is awaiting input; no blue tab flash on resize when idle
- **Split panes visible behind overlay** — Split panes now hide when an overlay tab (Git Panel, Settings, etc.) is active
- **Scroll position lost on fit** — Terminal scroll position is always restored after `fitAddon.fit()`
- **Repo watchers not started at runtime** — HEAD and repo watchers now start when adding a repository at runtime (not only on app launch)
- **Commit graph scope** — Graph follows HEAD only, matching the commit log scope
- **Plugin CORS** — `plugin://` protocol responses now include CORS headers for cross-origin access
- **Merged branch false positives** — Branches at the same SHA as main are excluded from the merged list
- **Intent body double space** — Intent body text trimmed after SGR strip to prevent leading/trailing whitespace
- **Plan-file notification spam** — Info sound no longer fires repeatedly from repeated plan-file detections
- **PTY output filtering** — Replaced `chrome_only` heuristic with per-row content check; replaced overly broad `)? ` filter with targeted code-try regex
- **Font inconsistency** — File list fonts harmonized to `--font-md` (13px) across all panels
- **Remote PR button overflow** — Button row layout and dismiss UX fixed in remote-only PR popover

### Performance
- **Branch select** — Three hot paths optimized in `handleBranchSelectInner`
- **File system** — `list_directory` and `search_files` made async; `search_files` rewritten with `ignore` crate for gitignore-aware walking; removed per-entry `canonicalize` overhead
- **Incremental compilation** — Enabled for release profile to speed up iterative builds
- **Git Panel IPC** — Suppresses fetch calls when panel is hidden

## [0.8.2] - 2026-03-11

### Added
- **TUIC_SESSION env var** — Every terminal tab gets a stable UUID injected as `TUIC_SESSION` in the shell. Use `claude --session-id $TUIC_SESSION` for tab-bound sessions that resume automatically on restart. Supported agents: Claude Code, Gemini CLI, Codex CLI
- **Git Operations Panel redesign** — Complete rewrite with 400px panel, rich status card (branch, ahead/behind, staged/changed/stash counts, last commit), background execution via `run_git_command`, inline feedback bar, searchable BranchCombobox, Create Branch form, rebase/cherry-pick in-progress UI, monochrome SVG icons, keyboard navigation (Escape to close, autofocus)
- **`get_git_panel_context` Tauri command** — Single IPC round-trip for all Git Operations Panel data (cached 5s TTL)
- **BranchCombobox shared component** — Searchable combobox with keyboard navigation for branch selection
- **File drag & drop** — Drag files from Finder/Explorer onto the terminal area to open them with the appropriate viewer (`.md`/`.mdx` → Markdown, others → Code Editor). Visual overlay during drag hover. Supports standalone files outside any repo
- **Markdown file association** — `.md`/`.mdx` files registered with TUICommander on macOS. Double-clicking a markdown file in Finder opens it directly in TUICommander
- **File browser: auto-refresh** — Directory watcher (notify crate) detects external file changes and refreshes automatically within ~1s, preserving selection by path
- **File browser: sort dropdown + UI polish** — Sort toggle replaced with compact inline dropdown (funnel icon) next to breadcrumb path. Parent row ("..") is now smaller and subtler
- **Claude Usage: rate-limit headers fallback** — Falls back to unified rate-limit response headers when per-model header data is unavailable, improving accuracy of the usage dashboard

### Changed
- **Updater: beta/nightly check moved to Rust** — `check_update_channel` replaces `fetch_update_manifest`. URLs hardcoded in Rust (SSRF-safe), 15s timeout, 64 KB size cap, typed results. TS store is now a pure state consumer with no URL constants or error regex
- **Post-merge cleanup: auto-stash** — Switch step now auto-stashes uncommitted changes instead of blocking. Dialog shows inline warning with optional "Unstash after switch" checkbox
- **Mobile Activity Feed: throttled grouping** — Items snapshot every 10s to prevent constant reordering with multiple active sessions
- **Claude Usage dashboard adaptive layout** — Dashboard adapts gracefully when only partial usage data is available, preventing empty columns

### Fixed
- **Ghost question notifications on PWA** — Question state now auto-clears when agent resumes work (status-line event)
- **Mobile table rendering** — Box-drawing characters preserve alignment via horizontal scroll (`white-space: pre`)
- **Mobile emoji rendering** — Unicode symbols (●, ○, ◉) forced to text presentation via `font-variant-emoji: text`
- **Updater CSP bypass** — Beta/nightly update manifest now fetched via Rust backend instead of the webview, bypassing CSP restrictions that blocked update checks. Missing channel releases shown as info, not error
- **Question detection reliability** — Stale pending state cleared so re-asked questions can refire; repaint-triggered false re-fires suppressed; screen-based detection via `last_chat_line` for accuracy; generalized prompt style recognition for Codex and Gemini
- **Rate-limit false positives** — Rate-limit events now gated on agent presence; terminals with no active agent no longer show spurious rate-limit badges
- **Rate-limit auto-expire** — Stale `rate_limited` state automatically clears after `retry_after_ms` elapses, preventing the badge from persisting beyond the actual limit window
- **Notification sound deduplication** — Plan-file info sound no longer fires multiple times per event; notification sounds decoupled from parsed event handlers to prevent double-firing
- **Sidebar auto-close** — Sidebar's focus-loss auto-close no longer dismisses the post-merge cleanup dialog mid-workflow

## [0.7.1] - 2026-03-08

### Changed
- **Notification sounds moved to Rust** — Audio playback moved from JS Web Audio API to Rust `rodio` crate. Eliminates AudioContext suspend issues on WebKit and works reliably in both Tauri and headless/remote modes
- **Transport table-driven mapping** — `mapCommandToHttp` refactored from 370-line switch to declarative `COMMAND_TABLE` for easier maintenance
- **Agent Teams simplified** — it2 shim infrastructure commented out; Agent Teams now uses env-var-only approach (`CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`) with direct MCP tool spawning
- **Mobile TerminalKeybar consolidated** — QuickActions merged into context-aware TerminalKeybar with agent-specific Yes/No buttons and Enter key for Ink TUI navigation
- **Intent token colorization** — Embedded ANSI codes from Ink renderers are now stripped from intent body text so dim-yellow color is uniform
- **Question detection simplified** — Removed challenged threshold; all silence-based questions use a single 10s timeout

### Fixed
- **Intent tokens visible in PWA** — `[[intent:...]]` and `[[suggest:...]]` structural tokens are now stripped from log lines served to PWA/REST consumers
- **PTY echo false question detection** — User-typed input echoed by PTY no longer triggers the silence-based question detector (500ms suppression window)
- **Headless reader question detection** — `extract_question_line` now applies to HTTP-created sessions, not just Tauri-spawned ones
- **Mobile input echo** — CommandInput sends `Ctrl-U` + text + Enter atomically to prevent duplicate echo
- **PluginManifest field naming** — TypeScript PluginManifest fields aligned to Rust serde camelCase serialization (`minAppVersion`, not `min_app_version`)
- **Notification subtask detection** — Parser now recognizes `⏵⏵` (U+23F5) prefix in addition to `››` (U+203A) for Claude Code active subtask counting; restored 10s notification deferral
- **Agent session lifecycle events** — MCP-spawned sessions now emit `session-created` and `session-closed` events so they appear as tabs and clean up correctly

## [0.7.0] - 2026-03-06

### Added
- **Markdown search** — `Cmd+F` now works in the markdown viewer with DOM-based text search, cross-element matching, highlight navigation, case/regex/whole-word toggles, and shared SearchBar component (also used by terminal search)
- **VT100 log extraction for mobile** — New `VtLogBuffer` per session uses a full VT100 parser to extract clean log lines from PTY output. Alternate-screen TUI apps (vim, htop, Claude Code) are suppressed — no garbled screen renders in mobile output. Accessible via `GET /sessions/:id/output?format=log` (returns `{lines, total_lines}`) and `WS /sessions/:id/stream?format=log` (catch-up on connect, then 200ms polling frames as `{type:log,lines:[...],offset:N}`). Mobile `OutputView` component now uses `format=log` for both initial fetch and live streaming
- **Session-aware agent resume** — When an agent is detected running in a terminal, TUICommander automatically discovers its session UUID from the filesystem and persists it per-terminal. On restore, uses the agent-specific `--resume <uuid>` for exact session matching. Supported: Claude Code (`~/.claude/projects/`), Gemini CLI (`~/.gemini/tmp/`), Codex CLI (`~/.codex/sessions/`). Multiple concurrent agents are handled via deduplication. Non-discoverable agents (Aider, Amp, etc.) fall back to their static resume commands. Context-menu "Launch Agent" now auto-executes the launch command without requiring a banner click
- **MCP bridge as Tauri sidecar** — `tuic-bridge` ships with the app and auto-configures MCP on first launch for Claude Code, Cursor, Windsurf, VS Code, Zed, Amp, Gemini
- **MCP `tools/list_changed` SSE notification** — Connected MCP clients receive live tool-list updates when upstream tool lists change
- **MCP Proxy Hub** — TUICommander now aggregates upstream MCP servers and exposes them through its own `/mcp` endpoint. Configure HTTP and stdio upstream servers in Settings > Services > MCP Upstreams; their tools are automatically available to any MCP client (Claude Code, Cursor, VS Code) connecting to TUIC. Features: tool namespace prefixing (`{upstream}__{tool}`), per-upstream tool allow/deny filters, circuit breaker (3 failures → open, 1s–60s exponential backoff, 10 retries → permanent failure), 60-second health checks, hot-reload on config save, credential storage via OS keyring, environment sanitization for stdio children, SSE status events, and self-referential URL detection
- **Worktree Manager panel** — Dedicated overlay (`Cmd+Shift+W` or Command Palette) listing all worktrees across repos with branch name, repo badge, PR state, dirty stats, and last commit timestamp. Features: orphan detection with Prune action, repo filter pills + text search, multi-select with batch delete and batch merge & archive, single-row actions (Open Terminal, Delete, Merge & Archive). Main worktrees have destructive actions disabled
- **Terminal CWD tracking via OSC 7** — Terminals detect working directory changes via OSC 7 escape sequences. When a terminal cd's into a known worktree, the tab automatically reassigns to that worktree's branch. Supports restart recovery via Rust-side cwd persistence.
- **Remote PTY session tab styling** — Sessions created via HTTP/MCP now display with amber tab color and "PTY:" name prefix for instant visual distinction from local terminals
- **Multi-agent status line detection** — Output parser now recognizes status lines from Claude Code (✢/·/asterisk), Aider (Knight Rider scanner + token reports), Codex CLI (bullet spinner), GitHub Copilot CLI (∴/●/○ indicators), Gemini CLI, Amazon Q, and Cline (braille dots). Tab titles update correctly for all supported agents
- **MCP workspace tool** — New `workspace` MCP tool with `list` (all open repos with groups, worktrees, branch, dirty status) and `active` (currently focused repo) actions
- **MCP notify tool** — New `notify` MCP tool with `toast` (temporary notification with info/warn/error level) and `confirm` (blocking confirmation dialog, localhost-only) actions
- **Plugin context menu actions** — Plugins can register custom actions in the terminal right-click "Actions" submenu via `host.registerTerminalAction()`. Actions receive a context snapshot (sessionId, repoPath) captured at right-click time, support dynamic `disabled` callbacks, and auto-cleanup on plugin unload. Requires new `ui:context-menu` capability
- **Plan Panel** (`Cmd+Shift+P`) — New right-side panel showing plan files for the active repository. Plans are detected from agent output via structured events, filtered by active repo, and auto-open as background tabs on first detection. Frontmatter is stripped from rendered content. Panel visibility and width persist across restarts
- **Agent Teams it2 shim** — Bash shim at `~/.tuicommander/bin/it2` emulates iTerm2 CLI for Claude Code Agent Teams. Supports `session split`, `run`, `close`, and `list`. PTY env injection sets `ITERM_SESSION_ID`, `TERM_PROGRAM`, and prepends shim to `PATH`. Enable via Settings > General > Agent Teams
- **Suggest follow-up actions** — Agents can propose follow-up actions via `[[suggest: ...]]` tokens. Desktop shows a floating chip bar (SuggestOverlay) with 30s auto-dismiss; mobile shows horizontal scrollable pills above CommandInput. Configurable in Settings > Agents
- **Mobile companion UI redesign** — Hero metrics header with active/awaiting counts, elevated session cards with rich sub-rows (intent, task, progress, usage), error and rate-limit info bars with live countdown, suggest follow-up chips, frosted glass bottom tabs, connection status in settings, 16px input font to prevent iOS auto-zoom
- **Quick Branch Switch** (`Cmd+B`) — Fuzzy-search dialog to switch branches instantly. Shows all local and remote branches for the active repo with current/remote/main badges. Remote branches auto-checkout as local tracking branches
- **Move terminal to worktree** — Right-click a terminal tab → "Move to Worktree" submenu to move the terminal to a different worktree. Also available via Command Palette with dynamic "Move to worktree: <branch>" entries
- **Customizable keybindings** — Click the pencil icon next to any shortcut in Help > Keyboard Shortcuts to rebind it. Conflict detection, per-shortcut reset, and "Reset all to defaults" button. Overrides persist in `keybindings.json`
- **Tip of the Day improvements** — Expanded from 18 to 31 tips covering all discoverable features. Larger fonts, brighter colors, sliding dot window (max 7 visible). Fixed click-through bug on arrows and dots
- **Post-merge cleanup dialog** — After merging a PR from the popover, a stepper dialog offers checkable steps: switch to base branch (with dirty state detection), pull (ff-only), delete local branch (closes terminals first), delete remote branch (handles "already deleted" gracefully). Steps execute sequentially via Rust backend (not PTY). Available from both local PR popover and remote-only PR popover. Also replaces the old MergePostActionDialog for worktree cleanup — when `afterMerge=ask`, the same unified dialog includes an archive/delete worktree step with an inline selector
- **Unseen terminal status dot** — Purple dot on terminals that completed work while the user was viewing a different terminal. Clears when the terminal is selected. Branch/worktree icons in the sidebar also show purple when containing unseen terminals
- **PR diff panel tab** — View Diff button in PR popover opens a dedicated panel tab with collapsible file sections, dual line numbers, and color-coded additions/deletions
- **Dismiss/Show Dismissed for remote-only PRs** — Hide irrelevant remote PRs from the sidebar; "Show Dismissed" toggle brings them back
- **Approve button for remote-only PRs** — Submit an approving review via GitHub API directly from the PR popover
- **Slash menu detection** — Output parser detects `/command` menus from screen bottom rows; mobile PWA renders a native bottom-sheet overlay for selection
- **GitHub merge method auto-detection** — Merge method selected from repo's allowed methods via GitHub API; auto-fallback to squash on HTTP 405 rejection
- **Mobile PWA enhancements** — TerminalKeybar (Ctrl+C/D/Tab/Esc/arrows), CLI command widget (agent-specific quick commands), offline retry queue for write_pty, session kill/new, search/filter in output, semantic log line colorization, slash menu overlay, connectivity indicator, isolated CSS (`mobile.css`), WebSocket state deduplication

### Changed
- **Progressive worktree loading** — `refreshAllBranchStats` now uses two-phase progressive loading. Phase 1 (`get_repo_structure`) returns worktree paths and merged branches instantly, so WorktreeManager rows appear immediately. Phase 2 (`get_repo_diff_stats`) fills in diff stats and timestamps progressively. Auto-archive of merged worktrees runs after Phase 1 instead of waiting for all stats
- **MCP cross-platform IPC transport** — MCP server uses Unix domain socket on macOS/Linux and named pipe (`\\.\pipe\tuicommander-mcp`) on Windows. `tuic-bridge` sidecar now works on all platforms. Bridge path is verified and updated on every app launch (not just first install)
- **MCP bridge path auto-update** — `ensure_mcp_configs()` runs on every launch, detects stale bridge paths in agent configs (from reinstalls, updates, or moves) and updates them automatically
- **MCP session output ANSI stripping** — MCP session output now strips ANSI codes by default (pass `format=raw` to preserve)

### Fixed
- **False question notification on user-typed input** — User-submitted lines echoed by PTY no longer trigger question detector
- **Voice dictation TOCTOU race on rapid start** — `compare_exchange` prevents duplicate recording sessions
- **Voice dictation final transcription accuracy** — Final transcription uses full captured audio instead of tail-only for improved accuracy
- **DictationToast lifecycle** — Removed duplicate event subscription causing stale toast state
- **macOS TCC permission prompts** — App was triggering "would like to access Desktop/Documents" dialogs due to filesystem probing in Claude Usage slug resolver, terminal path canonicalization, and file dialogs without defaultPath. All four code paths now guard against TCC-protected directories
- **Tab/sidebar animations not playing** — `pulse-opacity` keyframes defined in `global.css` were silently ignored by CSS Modules (scoped name mismatch). Moved keyframes into each module file; activity dots, busy indicators, and awaiting-input pulses now animate correctly
- **Rate-limit false positives** — Rate-limit pattern matches are now suppressed when the terminal is actively producing output (busy state), eliminating noise from agents reading code that contains rate-limit strings
- **Prompt Library focus loss** — Terminal now regains focus after prompt injection from the Prompt Library drawer
- **False positive API error** — Removed overly generic "request failed unexpectedly" from copilot-auth-error pattern, was triggering on normal Claude Code output

## [0.6.0] - 2026-02-28

### Added
- **Plugin filesystem write/rename** — New `fs:write` and `fs:rename` capabilities allow plugins to write and rename files within `$HOME` with path-traversal validation
- **Plugin panel message bridge** — `openPanel()` accepts `onMessage` callback for structured iframe→host messaging; `PanelHandle.send()` delivers host→iframe messages. Replaces fragile global `window.addEventListener("message")` pattern
- **Plugin panel CSS theme injection** — CSS custom properties (`--bg-*`, `--fg-*`, `--border*`, etc.) are automatically injected into plugin panel iframes, so plugins inherit the app theme without manual color copying
- **Auto-delete branch on PR close** — Per-repo setting (off/ask/auto) to automatically delete local branches when their GitHub PR is merged or closed. Handles worktree cleanup, dirty-state escalation, and main-branch protection
- **Worktree system overhaul** — Configurable storage strategies (sibling, app dir, inside-repo), three creation flows (dialog with base ref, instant, right-click quick-clone), hybrid branch naming (`{source}--{random}`), merge & archive workflow, external worktree detection via `.git/worktrees/` monitoring, per-repo worktree settings with global defaults
- **Centralized error log panel** — Ring-buffer logger captures all errors, warnings, and info from app, plugins, git, network, and terminal subsystems. Filterable overlay panel with level tabs, source dropdown, and text search. Status bar badge shows unseen error count. Keyboard shortcut: `Cmd+Shift+E`
- **Plugin log forwarding** — Plugin `host.log()` calls now appear in the centralized error log panel alongside app-wide logs
- **Agent-scoped plugins** — `agentTypes` manifest field restricts plugin output watchers and structured event handlers to terminals running specific agents (e.g. `["claude"]`). Universal plugins (empty array) continue to receive all events
- **File browser → Markdown viewer routing** — `.md`/`.mdx` files opened from the file browser now open in the Markdown panel instead of the code editor
- **Plugin CLI execution** — `exec:cli` capability allows plugins to run whitelisted CLI binaries (sandboxed: allowlist, timeout, stdout limit, trusted-directory validation)
- **Session prompt tracking** — Built-in `sessionPromptPlugin` reconstructs user-typed input from PTY keystrokes and displays in Activity Center
- **Input line buffer** — Rust-side virtual line editor (`input_line_buffer.rs`) reconstructs typed input from raw PTY keystroke data, supporting cursor movement, word operations, and Kitty protocol sequences
- **mdkb Dashboard plugin** — External installable plugin for viewing mdkb knowledge base status, memories, and configuration
- **API error detection** — Output parser detects API errors (5xx, auth failures) from agents (Claude Code, Aider, Codex CLI, Gemini CLI, Copilot) and provider-level JSON error formats (OpenAI, Anthropic, Google, OpenRouter, MiniMax). Triggers error notification sound and logs to centralized error panel
- **Rust-backed log ring buffer** — Warn/error entries survive webview reloads via `push_log`/`get_logs` Tauri commands
- **Switch Branch submenu** — Main worktree context menu with dirty-tree stash prompt and running-process guard
- **Merged badge** — Branches merged into main show a "Merged" badge in the sidebar
- **Info notification sound type** — Added "info" to per-event notification sounds
- **Tab bar overflow menu** — Right-click scroll arrows to see clipped tabs; `+` button always stays visible
- **Focus-aware dictation** — Transcribed text inserts into focused input element instead of always targeting terminal PTY
- **Auto-fetch interval** — Per-repo setting to periodically `git fetch --all` in the background (5/15/30/60 min), keeping branch stats and ahead/behind counts fresh without manual intervention
- **LLM intent declaration** — Agents emit `[[intent: <action>]]` tokens that the output parser captures and displays in the Activity Dashboard, showing real-time work intent alongside user prompts
- **Mobile Companion UI** — Phone-optimized PWA at `/mobile` for monitoring AI agents remotely. Session list with status cards, live output with quick-reply chips, question overlay banner, activity feed, notification sounds. Installable via Add to Home Screen on iOS Safari and Android Chrome
- **Streaming dictation with VAD** — Real-time partial transcription during push-to-talk via adaptive sliding windows (1.5s→3s). Voice Activity Detection energy gate skips silence to prevent hallucinations. Floating toast shows partial text above status bar. No new dependencies — built entirely on whisper-rs

### Changed
- **`get_repo_summary` single-IPC** — New Rust command collapses worktree paths + merged branches + per-path diff stats into one round-trip, replacing N+2 separate IPC calls in `refreshAllBranchStats`
- **RPC deduplication** — Concurrent identical idempotent (GET) RPC calls are coalesced into a single in-flight request
- **StatusBar shared timer** — Merged two separate 1-second intervals (rate-limit countdown + PR grace period) into one
- **Terminal resize cleanup** — Removed redundant Tauri window resize listener (ResizeObserver already handles this)
- Agent session restore now shows a clickable banner instead of auto-injecting the resume command
- Migrated ~200 `console.error`/`console.warn` calls to centralized `appLogger` across terminal, hooks, stores, UI components, plugins, and utilities (waves 1-4)
- Activity Dashboard shows last user prompt (>= 10 words) as sub-row with tooltip, now native Rust implementation
- OSC 8 hyperlinks in terminal now open in system browser correctly

### Fixed
- Worktree removal now respects the `deleteBranchOnRemove` setting instead of always deleting the local branch
- File path link underline no longer flickers on mouse hover (cached link provider)
- Rate limit and usage limit badges no longer trigger redundantly on terminal resize
- Terminal focus no longer silently switches to a terminal from another repo
- Rapid branch switching no longer creates duplicate terminals (serialization lock)
- HEAD-changed events during branch rename no longer lose terminal state
- Push-to-talk race condition — fast key release no longer drops transcription
- Claude usage timeline gaps — flush orphan tokens from active sessions
- Merged branch detection hardened with file I/O probing and 5s TTL cache
- **Activity Dashboard state inconsistencies** — `setActive()` no longer resets `shellState` to null; busy flag reconciliation on every PTY chunk prevents "—" status for working terminals; agent polling now covers all terminals (not just the active one)
- **Rate-limit false positives** — Added `line_is_source_code()` guard so agents reading `output_parser.rs` no longer trigger their own rate-limit patterns
- **False "awaiting input" indicator** — Silence-based question detector threshold raised from 5s to 10s; added `line_is_likely_not_a_prompt()` guard to filter code, markdown, and long lines
- **Output parser false positives** — Status line detection now skips diff output, code listings, and block comments; intent parsing requires line-start/whitespace anchor; rate limit and API error detection uses ANSI-stripped text to prevent escape-code bridging (e.g. "story 429" no longer triggers HTTP 429 detection)

### Removed
- `showAllBranches` toggle (replaced by Switch Branch submenu)
- `sessionPromptPlugin` built-in (replaced by native Rust last-prompt tracking)

### Documentation
- FEATURES.md: documented tab pinning, branch sorting, Kitty keyboard protocol, PTY pause/resume, MCP registration with Claude CLI

### Security
- **Plugin exec binary resolution hardened** — Removed `which`/`where` PATH lookup; binary resolution now uses only hardcoded trusted directories with symlink canonicalization to prevent symlink attacks
- **Plugin exec stderr truncated** — Error messages from failed CLI commands now truncate stderr to 256 bytes to prevent leaking secrets

### Housekeeping
- **Removed dead wizStoriesPlugin built-in** — Extracted to external plugin; orphaned source and tests cleaned up
- **Replaced wiz-specific example plugins** — `wiz-stories` and `wiz-reviews` examples replaced with generic `report-watcher` and `claude-status` (demonstrates agentTypes)
- **Ideas audit** — Reclassified 4 ideas: PR Merge Readiness → done, Worktree Status Refresh → done (implemented via revision-based reactivity), Structured Agent Output → rejected (requires upstream adoption), Analytics/Editor Settings clarified (editors done, analytics deferred)
- **Plugins submodule updated** — registry.json and README cleaned up, mdkb-dashboard added

### Planned
- **Tab scoping per worktree** — Each worktree/branch will have its own isolated set of tabs instead of sharing a global tab list

### Infrastructure
- **Nightly workflow: move tip tag** — Cleanup job now force-moves the `tip` git tag to the current commit before building, so the release always points to HEAD
- **Makefile: unified CI targets** — Replace `build-github-release` / `publish-github-release` / old `github-release` with two clean targets: `make nightly` (push + tip tag) and `make github-release BUMP=patch` (version bump + tag + CI + publish)
- **Makefile: github-release fixes** — `cargo check` stderr no longer suppressed; run ID lookup matches by commit SHA to avoid race conditions

---

## [0.5.4] - 2026-02-24

### Terminal

- **Ghostty terminal identity** — Switch from kitty to ghostty for Claude Code's terminal detection allow-list (CC v2.1.52 compatibility)
- **Shift+Enter multi-line input** — Sends `\x1b\r` (ESC+CR) for multi-line newlines in Claude Code and other CLI apps
- **Shift+Tab focus fix** — Prevents browser focus navigation while letting xterm send CSI Z to PTY
- **Kitty flags initial sync** — Race condition fix: query kitty flags on listener attach to avoid missed push events
- **Tab close focus transfer** — Closing the active tab now properly focuses the next tab via `handleTerminalSelect` (includes `ref.focus()`)

### Infrastructure

- **Transport layer compliance** — `get_kitty_flags` routed through `usePty`/`transport.ts` with HTTP handler for browser mode
- **Linux CLI resolution** — Added `/usr/bin` to `extra_bin_dirs` for minimal desktop environments
- **Nested session guard** — `env_remove("CLAUDECODE")` prevents "cannot launch inside another CC session" error

### Fixed

- **Windows clippy errors** — Unused variables and collapsible ifs
- **rAF close-all guard** — Prevent crash when concurrent tab closes race with deferred focus callback

---

## [0.5.0] - Unreleased

### Plugin System

- **External plugin loading** — Plugins live in `~/.config/tui-commander/plugins/{id}/` and are loaded at runtime via the `plugin://` URI scheme; hot reload when files change on disk
- **Plugin Settings tab** — Install plugins from a ZIP file or URL, enable/disable, uninstall, view per-plugin logs
- **Community registry / Browse tab** — Discover and install plugins from `sstraus/tuicommander-plugins`; 1-hour TTL cache with manual refresh
- **`tuic://` deep link scheme** — `tuic://install-plugin?url=…`, `tuic://open-repo?path=…`, `tuic://settings?tab=…`
- **Per-plugin error logging** — 500-entry ring-buffer logger per plugin; errors from lifecycle hooks and watchers captured automatically
- **Capability-gated PluginHost API** — Tier 1 (activity/watchers), Tier 2 (read-only state), Tier 3 (PTY write, markdown panel, sound), Tier 4 (whitelisted Tauri invoke)
- **Built-in plugin toggle** — Plan and Stories plugins can be disabled from Settings → Plugins
- **Activity Center bell** — Toolbar bell replaces the plan button; plugins contribute sections and items; supports per-item dismiss and "Dismiss All"
- **4 sample plugins** in `examples/plugins/` demonstrating all capability tiers
- **Plugin filesystem API** — `fs:read`, `fs:list`, `fs:watch` capabilities for sandboxed file access within `$HOME` (10 MB limit, glob filtering, debounced watching via `notify`)
- **Plugin data HTTP endpoint** — `GET /api/plugins/{id}/data/{path}` exposes plugin data to external HTTP clients

### Terminal

- **Detachable terminal tabs** — Float any terminal tab into an independent OS window; re-attach on close
- **Find in Terminal** (`Cmd+F`) — In-terminal search overlay with match count and navigation
- **Configurable keybindings** — Remap any shortcut in Settings → Keyboard Shortcuts; persisted to `~/.config/tui-commander/keybindings.json`
- **iTerm2-style Option key split** — macOS: left Option sends Meta (for Emacs/readline), right Option sends special chars; configurable per repo
- **Per-repo terminal meta hotkeys** — Override Option key behavior per repository in Settings

### Settings Panel

- **Split-view layout** — Vertical nav sidebar + content pane replaces the old dialog
- **Repos in Settings nav** — Each repo appears as a nav item with deep-link open support
- **Keyboard Shortcuts tab** — Browse and rebind all app actions
- **About tab** — App version, links, acknowledgements
- **Appearance tab** — Absorbs former Groups tab; theme, color, font settings in one place
- **Global repo defaults** — Set base branch, color, and other defaults; per-repo settings override only what differs

### File Browser & Editor

- **File browser panel** (`Cmd+E`) — Tree view of the active repository with git status indicators, copy/cut/paste, context menu
- **CodeMirror 6 code editor** — Full editor panel with tab system, syntax highlighting, and file browser integration
- **Markdown edit button** — Pencil icon in MarkdownTab header opens the file in the code editor
- **Clickable file paths** — File references in diff and code panels open in the editor or focused in the IDE
- **Panel search** — Search within code and diff panels
- **Mutually exclusive panels** — File browser, Markdown, and Diff panels are now mutually exclusive to save screen space
- **Drag-resize** — Panel dividers are draggable

### Git & GitHub

- **Diff panel commit dropdown** — Select any recent commit to diff against; Working / Last Commit scope toggle
- **PR notification rich popover** — Click the bell to see PR title, CI status, review state, and open in browser
- **Plan file detection** — Toolbar button lights up when an agent creates a plan file in the active repo
- **GitHub API rate limit handling** — Graceful backoff and UI indicator when GitHub API rate limit is hit

### Agent Support

- **New agents** — Amp, Jules, Cursor, Warp, Ona; brand SVG logos for all supported agents
- **Silence-based question detection** — Recognizes interactive prompts for unrecognized agents via output silence heuristic
- **MCP tools consolidation** — 21 individual MCP tools replaced by 5 meta-commands

### Cross-Platform

- **Windows compatibility** — Platform-aware shell escaping (cmd.exe vs POSIX), foreground process detection via `CreateToolhelp32Snapshot`, Windows paths in `resolve_cli`, IDE detection/launch, `if exist` syntax for lazygit config detection

### Other Added

- **Command Palette** (`Cmd+P`) — Fuzzy search across all app actions with recent-first ordering
- **Activity Dashboard** (`Cmd+Shift+A`) — Real-time view of all terminal sessions and agent status
- **Park Repos** — Right-click any repo to park it; sidebar footer button shows parked repos with badge count
- **Repository groups context menu** — Right-click any repo to "Move to Group" with "New Group..." option
- **Lazy terminal restore** — Terminal sessions materialize only when clicking a branch, not on startup
- **Check for Updates menu** — In both app menu and Help menu
- **Repo watcher** — Shared file watcher for automatic panel refresh on `.git/` changes
- **Context menu submenus** — ContextMenu supports nested children
- **Remote access QR code** — Shows actual local IP address; HTTPS-only install links; firewall reachability check
- **Auto-hide closed/merged PRs** — PR notifications for closed or merged PRs are automatically dismissed

### Changed

- **Display name** — "TUI Commander" renamed to "TUICommander" across the codebase
- **UX density** — Tighter status bar (22px), toolbar (35px macOS), and sidebar row spacing to match VS Code density
- **Browser/remote mode** — Full compatibility with MCP session events, CORS for any origin, IPv4 binding
- **Status bar icons** — All text labels replaced with monochrome SVG icons; buttons reordered
- **HelpPanel** — Simplified to app info and resource links; keyboard shortcuts moved to Settings
- **Sidebar design** — Flat layout; harmonized git actions and footer; SVG branch/asterisk icons
- **Tab creation UX** — `+` button creates new tab; split options on right-click only
- **CLI resolution** — All `git` and `gh` invocations route through `resolve_cli()` for reliable PATH in release builds
- **Diff panel shortcut** — Remapped from `Cmd+D` to `Cmd+Shift+D`
- **Data persistence guard** — `save()` blocks until `hydrate()` completes to prevent wiping `repositories.json`

### Fixed

- **Lazygit pane ghost terminal** on close
- **xterm fit() minimum dimensions** — Guard prevents crash on zero-size terminal
- **Terminal reattach fit** after floating window closes
- **Splash screen timing** — Deferred removal until stores are fully hydrated
- **Markdown viewer refresh** — Viewer now refreshes after saving a file in the code editor
- **Window-state corruption** — Guard against zero-dimension or off-screen persisted state causing PTY garbage
- **IDE detection in release builds** — `resolve_cli` probes well-known directories
- **Multi-byte UTF-8 panic** — Fixed in rate-limit debug output
- **International keyboard support** — Correct handling of intl input; fewer rate-limit false positives
- **Tab drag-and-drop** — Fixed by working around Tauri's internal drag handler
- **Left Option key state leak** — Reset on `altKey=false` to prevent stuck Meta state
- **PromptDialog hidden on mount** — Dialog now shows correctly when first rendered
- **Browser-mode init freeze** — Fixed hang when session cookie expires
- **Silent failures and memory leak** — P1 issues resolved (floating promises, missing cleanup)
- **Drag-over visual feedback** — Group sections show drop indicator during drag
- **Tab store mutual exclusivity** — Fixed markdown wheel scroll by enforcing only one tab store active at a time
- **Browser mode PTY creation** — Fixed ConnectInfo extraction and keybinding conflicts in remote mode

---

## [0.3.0] - 2026-02-19

### Added
- **Auto-update** - Check for updates on startup via tauri-plugin-updater, download progress badge in status bar, one-click install and relaunch
- **Prevent system sleep** - keepawake integration prevents sleep while agents are working (configurable in Settings)
- **Usage limit badge** - Detects Claude Code "You've used X% of your weekly/session limit" messages and displays a color-coded badge in status bar (blue < 70%, yellow 70-89%, red pulsing >= 90%)
- **Ideas panel** - Renamed Notes to Ideas with lightbulb icon, send-to-terminal and delete actions
- **Terminal session persistence** - Terminal sessions survive app restarts, with activeRepoPath live-sync
- **GitHub GraphQL API** - Replaced `gh pr list` CLI with direct GraphQL for PR statuses, CI checks, and token resolution
- **HEAD file watcher** - Watches `.git/HEAD` for branch changes instead of polling
- **Build & release targets** - Makefile targets for `build-github-release` and `publish-github-release`

### Changed
- **Git status via file reads** - Read branch and remote URL from `.git` files instead of subprocess for better performance
- **Status bar overflow** - Handles long content gracefully
- **Color picker** - Added to settings for theme customization
- **Default theme** - Changed to VS Code Dark, reordered theme lists

### Fixed
- **Empty GitHub token** - Filter empty strings from `gh_token` crate, fall back to `gh auth token` CLI
- **Agent resume commands** - Updated resume commands for OpenCode and Aider
- **Download progress bar** - Fixed layout in Dictation Settings
- **PTY environment** - Set `TERM=xterm-256color`, `COLORTERM`, and `LANG` for proper color and UTF-8 support
- **Branch name overflow** - Text ellipsis on long branch names in sidebar
- **Branch name styling** - Font size and color consistency
- **Worktree button** - Disabled during creation to prevent double-clicks
- **CI builds** - Linux `libasound2-dev` dependency, macOS notarization, Windows process group guard

---

## [0.2.0] - 2026-02-18

### Added
- **Terminal context menu** - Split right/left/down/up, reset terminal, change title
- **PR state badges** - Replace CI ring with merge/review state badges in sidebar
- **PR clickable links** - PR number opens GitHub in browser
- **Rate limit warning** - Badge in status bar when AI agents hit rate limits
- **Question detection** - Recognizes interactive prompts in terminal, shows ? icon in sidebar
- **Dock badge count** - macOS dock badge for attention-requiring notifications
- **Auto-show PR popover** - Optional setting to auto-display PR details
- **Splash screen** - Branded loading screen on app start
- **Repo header context menu** - Right-click on repo header in sidebar
- **Smart branch terminal spawn** - Auto-spawns terminal only on first branch select; respects user intent when all tabs are closed

### Changed
- **Design system tokens** - Migrated all hardcoded CSS values to CSS custom properties
- **WCAG AA compliance** - Theme-aware text-on-color system, contrast fixes across all UI elements
- **Standardized sizing** - Consistent button, badge, and input dimensions
- **Sci-fi worktree names** - More creative auto-generated worktree names
- **OSC title cleaning** - Filters shell script noise and extracts useful command names
- **Lazygit tab naming** - Explicitly sets tab name to avoid polluted OSC titles
- **Tauri webview build targets** - Optimized build configuration

### Fixed
- **macOS "Restored session" message** - Suppressed by setting TERM_PROGRAM in PTY
- **PR popover layout** - Improved readability and positioning
- **Hotkey macOS symbols** - Correct modifier symbol translation
- **Sidebar badge layout** - Proper alignment and spacing
- **MCP HTTP port conflict** - Server falls back to localhost on port conflict
- **WebGL canvas fallback** - Graceful degradation when WebGL addon fails
- **ErrorBoundary crash screen** - Shows recovery UI instead of blank screen

---

## [0.1.0] - 2026-02-04

### Added
- Initial TUICommander implementation
- **Multi-terminal support** - Up to 50 concurrent PTY sessions
- **Repository sidebar** - Hierarchical view of repositories with branches and worktrees
- **Tab bar** - Terminal tabs with keyboard shortcuts (Cmd+1-9)
- **Git worktree integration** - Create and manage git worktrees from the UI
- **Per-pane zoom** - Independent font size control per terminal (Cmd+Plus/Minus)
- **VS Code / Claude Code launchers** - Quick access buttons in status bar
- **Markdown preview panel** - Toggle with MD button
- **Diff preview panel** - Toggle with Diff button
- **Session persistence** - Terminals maintain state across tab switches
- **Branch-terminal association** - Terminals are tracked per branch

### Architecture
- **Frontend**: SolidJS + TypeScript + Vite
- **Backend**: Tauri (Rust) for native performance
- **Terminal**: xterm.js with WebGL renderer
- **State Management**: Custom stores (terminals, repositories)

### Known Issues
- Tabs from all worktrees visible when switching branches (fix planned)

- Add Telegram setup in desktop and mobile Settings: token check/replacement, expiring one-use pairing or explicit chat IDs, live agent selection and safe daemon status.
