export interface Tip {
	feature: string;
	description: string;
	shortcut: string | null;
}

import { isMacOS } from "../platform";

const mod = isMacOS() ? "Cmd" : "Ctrl";

export const TIPS: Tip[] = [
	{
		feature: "Native story notes",
		description:
			"Select a story to read Notes and evidence or append a permanent note. Refresh notes loads entries added by agents or other windows without changing the story revision.",
		shortcut: null,
	},
	{
		feature: "Story origin and history",
		description:
			"Select a native story to inspect its origin, claiming session and recorded transitions. New story can link the work to a plan step.",
		shortcut: null,
	},
	{
		feature: "Edit native stories",
		description:
			"Select an unclaimed Backlog or Ready story and choose Edit to change metadata, requirements and dependencies together. Save checks the original revision; Cancel discards the draft. QA and active work cannot be amended.",
		shortcut: null,
	},
	{
		feature: "Native story QA",
		description:
			"Prefix human checks with [MANUAL]. When development checks pass, the story enters QA; use the QA filter or Kanban column to find checks that wait for a person.",
		shortcut: null,
	},
	{
		feature: "Mobile session usage",
		description:
			"Open More session actions on mobile to read the session profile's account quota or ego session tokens and cost. Unknown and stale usage are labelled explicitly.",
		shortcut: null,
	},
	{
		feature: "Telegram sender replies",
		description:
			"Agents can send Telegram notices without taking inbound ownership. Reply to a sender-prefixed notice in Telegram to reach that live agent; new messages still reach the inbound owner.",
		shortcut: null,
	},
	{
		feature: "Abandoned shells",
		description:
			"Plain shells with no running processes are marked abandoned after one idle hour. Settings → Agents → Idle agent hibernation has an expert setting to close them after a chosen number of hours; automatic closure is off by default.",
		shortcut: null,
	},
	{
		feature: "Session identifiers",
		description:
			"Right-click a sidebar session row to copy its PTY session ID, live child process ID, or MCP alias. Hover an entry to see its value.",
		shortcut: null,
	},
	{
		feature: "Mobile font sizes",
		description:
			"Use the Terminal and Chat controls in mobile Settings → Appearance to set each reading size independently. The choices stay on this device; composers and UI controls do not change.",
		shortcut: null,
	},
	{
		feature: "Launch playbook modules",
		description:
			"In Settings > Agents, select launch modules and add global or per-agent instructions without copying generated rules. Changes apply to new launches.",
		shortcut: null,
	},
	{
		feature: "All settings inspector",
		description:
			"Open Settings → All settings (Expert) for typed catalogue fields without a dedicated page. Select the owner and existing record key, then read before saving. Credentials stay in their domain workflows.",
		shortcut: null,
	},
	{
		feature: "ego terminal Chat view",
		description:
			"Switch an ego terminal from CLI to Chat to read completed replies and folded tool calls. Configure an ego executable with local transcript v1 support first; use CLI for interactive prompts.",
		shortcut: null,
	},
	{
		feature: "Mobile latest output",
		description:
			"When you scroll back in mobile CLI or Chat output, tap the down-arrow above Send to return to the latest output without sending a terminal key.",
		shortcut: null,
	},
	{
		feature: "AI Chat table actions",
		description:
			"Use Markdown to inspect a table's exact source, Copy as Markdown to copy it, or Open as document to read it outside the conversation.",
		shortcut: null,
	},
	{
		feature: "Hands-free conversation shortcut",
		description: `Press ${mod}+Alt+V to start or stop hands-free on the active terminal when dictation is enabled. Customize the chord in Keyboard Shortcuts.`,
		shortcut: `${mod}+Alt+V`,
	},
	{
		feature: "Tab shortcut hints",
		description: `Hold ${mod} alone for 450 ms to show shortcuts on the first nine terminal tabs. Release it or use another key, mouse button, or scroll to hide the hints.`,
		shortcut: null,
	},
	{
		feature: "Delete AI Chat conversations",
		description:
			"Open ⋯ → Delete conversation… in AI Chat and use a row’s trash button to permanently delete its history after confirming the name.",

		shortcut: null,
	},
	{
		feature: "ego provider recovery",
		description:
			"When terminal ego reports a provider outage, the status bar shows its recovery attempt and next retry. Select Force retry to send /retry once to that terminal.",
		shortcut: null,
	},
	{
		feature: "Idle agent hibernation",
		description:
			"Settings → Agents can hibernate unattended agent tabs after awake idle time. Open a hibernated tab or send it input to resume its saved session.",
		shortcut: null,
	},
	{
		feature: "Follow Tasks comments",
		description:
			"Follow an item in Tasks detail, then enable OS notifications for Tasks in Settings → Notifications. Existing detail refreshes notify for new comments; no extra background polling is added.",
		shortcut: null,
	},
	{
		feature: "Tasks local queries",
		description:
			"Tasks loads complete scopes of at most 1,000 items for all-field sorting, grouping and filtering. Unknown or larger totals use the provider-supported server controls.",
		shortcut: null,
	},
	{
		feature: "Claude Chat on mobile",
		description:
			"Select Chat in a Claude session header to read the conversation. Use the same input to reply, and switch to CLI for prompts missing from Chat.",
		shortcut: null,
	},
	{
		feature: "Plugin settings",
		description:
			"Find loaded plugin settings under Integrations, or search for a field in Settings. Plugin pages disappear when the plugin unloads.",
		shortcut: null,
	},
	{
		feature: "Desktop Markdown on mobile",
		description:
			"Open a session’s More actions menu on your phone to read the Markdown files open on desktop for that repository.",
		shortcut: null,
	},
	{
		feature: "Mobile composer actions",
		description:
			"Tap + beside a mobile draft to attach a file, take a photo, or use the draft and queue actions supported by that conversation.",
		shortcut: null,
	},
	{
		feature: "Mobile keyboard correction",
		description:
			"In the mobile Settings tab, use the Input section to disable autocorrect and spellcheck for literal terminal command entry. The choice stays on your device.",
		shortcut: null,
	},
	{
		feature: "Close to background",
		description:
			"In desktop Settings > General, choose Ask, Background or Quit for main-window close. Background keeps sessions running. The tray or menu-bar icon shows working and awaiting agents and offers Show and Quit.",
		shortcut: null,
	},
	{
		feature: "Compose images",
		description:
			"Drop images on Compose or use Attach images beside Send to choose several. Paste still works; image paths appear in your draft.",
		shortcut: null,
	},
	{
		feature: "Tasks view memory",
		description:
			"Tasks remembers your source, view, grouping, sort and filters separately for each repository and provider account.",
		shortcut: null,
	},
	{
		feature: "Tasks board",
		description:
			"Use More in any lane to load the next board page. Select Move… on a card to change its lane without dragging.",
		shortcut: null,
	},
	{
		feature: "Terminal Chat answers",
		description:
			"Hover or focus an answer card to copy the whole reply. Pasted terminal output shows two lines; select + N lines to expand it.",
		shortcut: null,
	},
	{
		feature: "Agent toast cards",
		description:
			"Normal agent notices show a card only on their active origin terminal tab; other tabs receive bell entries only. Turn off Show cards from the active terminal in Settings > Notifications to keep only urgent cards.",
		shortcut: null,
	},
	{
		feature: "New chat text",
		description:
			"While reading earlier chat messages, select New text below to return to the latest reply. Toggle the indicator in Settings > Appearance > Chat.",
		shortcut: null,
	},
	{
		feature: "Mobile prompt queue",
		description:
			"Hold Send and select Queue to submit your current agent draft at its next ready window. Tap Send to submit now.",
		shortcut: null,
	},
	{
		feature: "Tasks roadmap",
		description:
			"Switch Month or Quarter in the Tasks roadmap to change the timeline scale. Tasks without usable dates stay in the side list.",
		shortcut: null,
	},
	{
		feature: "Design Mode",
		description:
			"Right-click a terminal web link and choose Start in Design Mode to connect that agent and open the page directly. Pick elements and comment in the browser sidebar before sending.",
		shortcut: null,
	},
	{
		feature: "Spoken replies",
		description:
			"Turn off Spoken replies in mobile hands-free or Settings > Voice to keep dictating while the agent answers in text.",
		shortcut: null,
	},
	{
		feature: "Mobile Control Keys",
		description:
			"Tap Ctrl in the mobile terminal keybar to send Ctrl+C, Ctrl+B, Ctrl+D or the agent-aware Ctrl+Enter key.",
		shortcut: null,
	},
	{
		feature: "Automations",
		description:
			"Open Automations from the command palette to edit schedules. Enable Include precheck stdout in the agent prompt to add bounded check results before your unchanged prompt. Select Runs for history and saved output.",
		shortcut: null,
	},
	{
		feature: "Terminal Chat Input",
		description:
			"Switch a Claude terminal to Chat to reply in docked Compose. Ctrl+Enter sends now; Shift+Ctrl+Enter queues. Answer permission prompts in CLI.",
		shortcut: null,
	},
	{
		feature: "ego Launch Permissions",
		description:
			"Choose permission mode and sandbox in Settings > Agents > ego for new terminal launches and managed spawns.",
		shortcut: null,
	},
	{
		feature: "Launch Instruction Inspector",
		description:
			"Right-click a terminal and choose Inspect Launch Instructions to read captured launch sections, sources and byte sizes.",
		shortcut: null,
	},
	{
		feature: "Conversation Launch Options",
		description:
			"Use ⋯ → New conversation with options in AI Chat to give one conversation its own ego profile, workspace and executable.",
		shortcut: null,
	},
	{
		feature: "AI Chat Repository Pin",
		description:
			"Use Pin repository beside the AI Chat composer to keep that conversation on one repository while you browse elsewhere. Draft parking stays separate.",
		shortcut: null,
	},
	{
		feature: "Repository Ego Profiles",
		description:
			"Select ego_profile in a workspace's .tuic.json. Ego restricts it to your explicit machine profile and AI Chat shows any clamp warnings.",
		shortcut: null,
	},
	{
		feature: "ego Perimeter",
		description:
			"Use Settings > AI Chat to restrict ego roots, edit read and write directories, and inspect ego's effective perimeter before starting a new conversation.",
		shortcut: null,
	},
	{
		feature: "Child Agent Navigation",
		description:
			"In the rich sidebar, click the agent count below a parent to open its children in another repository.",
		shortcut: null,
	},
	{
		feature: "Touch Branch Actions",
		description: "Swipe a sidebar branch left for More, Add Terminal, and worktree removal. Hold + to choose an agent.",
		shortcut: null,
	},
	{
		feature: "Private Secret Forms",
		description:
			"Ask an agent to request named credentials in a private form. Review the exact command before allowing it to use them.",
		shortcut: null,
	},
	{
		feature: "AI Chat Prompt Parking",
		description:
			"Park a half-written AI Chat prompt to ask something else; it returns after your next Send. On phones, tap Park.",
		shortcut: "Ctrl+S",
	},
	{
		feature: "Mobile Session Search",
		description:
			"On the phone, tap the magnifier above Sessions to find an agent by name, repository, branch, or type.",
		shortcut: null,
	},
	{
		feature: "Parallel AI Chats",
		description: "With AI Chat focused, open another tab for a separate conversation and draft in the same repository.",
		shortcut: `${mod}+T`,
	},
	{
		feature: "AI Chat Images",
		description: "Paste an image into AI Chat, review its preview, then send it with or without text.",
		shortcut: `${mod}+V`,
	},
	{
		feature: "Open Editor Links",
		description: `Hold ${mod} and click a web link or file path in the code editor to open it.`,
		shortcut: `${mod}+Click`,
	},
	{
		feature: "Remote Machine Updates",
		description:
			"Remote Machines shows when a connected daemon is out of date. Update & restart confirms the live sessions that will close.",
		shortcut: null,
	},
	{
		feature: "Editor Line Wrapping",
		description: "Press Alt+Z in the editor to wrap long lines. Text and code files remember separate choices.",
		shortcut: "Alt+Z",
	},
	{
		feature: "Plans and Stories",
		description:
			"Open Plans and Stories from the project toolbar. On desktop, use its header to detach it into a resizable window.",
		shortcut: null,
	},
	{
		feature: "Workflow Run History",
		description: "Open Run history in Plans and Stories to follow a plan's persisted workflow timeline.",
		shortcut: null,
	},
	{
		feature: "Command Palette",
		description: "Search and run any action by name — shortcuts, settings, panels, everything.",
		shortcut: `${mod}+P`,
	},
	{
		feature: "Find Files",
		description:
			"Type ! in the command palette to search files by name. Type ? to search file contents. Results open in the editor.",
		shortcut: `${mod}+P then ! or ?`,
	},
	{
		feature: "Open an Agent Tab",
		description:
			"Hold the + in the tab bar to pick an agent. The new tab starts it directly instead of opening a shell.",
		shortcut: null,
	},
	{
		feature: "Agent Scrollback",
		description:
			"Agent commands in new terminals keep native scrollback when supported. Change Prevent alternate screen per agent in Settings.",
		shortcut: null,
	},
	{
		feature: "Activity Dashboard",
		description: "See all agent sessions, CPU usage, and errors in one panel.",
		shortcut: `${mod}+K`,
	},
	{
		feature: "Terminal Context",
		description:
			"Expand Context above an agent terminal to see its current intent, assigned task, and last submitted prompt.",
		shortcut: null,
	},
	{
		feature: "Design Mode",
		description:
			"Right-click an agent terminal tab to inspect an element in Chrome and paste its details into that agent's input.",
		shortcut: null,
	},
	{
		feature: "Split Panes",
		description: "Split the current terminal into two side-by-side panes.",
		shortcut: `${mod}+\\`,
	},
	{
		feature: "Quick Branch Switch",
		description: "Fuzzy-search all branches and switch instantly.",
		shortcut: `${mod}+B`,
	},
	{
		feature: "Reopen Closed Tab",
		description: "Bring back the last tab you closed — remembers up to 10.",
		shortcut: `${mod}+Shift+T`,
	},
	{
		feature: "Terminal Zoom",
		description: "Full-screen the active terminal, hiding sidebar and toolbar chrome.",
		shortcut: `${mod}+=`,
	},
	{
		feature: "Fullscreen App Scrollback",
		description:
			"Fullscreen apps keep a separate scrollback. Hold Shift while using the wheel, or drag the scrollbar, when the app captures mouse input.",
		shortcut: "Shift+wheel",
	},
	{
		feature: "File Browser",
		description: "Browse repo files in a side panel with live diff preview.",
		shortcut: `${mod}+E`,
	},
	{
		feature: "Plugin File Previews",
		description:
			"Install preview plugins from Settings > Manage plugins > Browse to open DOCX and XLSX files or inspect, filter, explain, and edit SQLite databases directly from the File Browser.",
		shortcut: null,
	},
	{
		feature: "File Content Search",
		description:
			"Press C in the file browser search bar to switch to content search — searches inside files with regex, case-sensitive, and whole-word options.",
		shortcut: `${mod}+Shift+F`,
	},
	{
		feature: "Prompts",
		description:
			"Open the Prompts dropdown and search Configure for guided issue-to-draft-PR, incoming PR review and authorized bounded-fix setup. The guides verify prerequisites before configuration.",
		shortcut: `${mod}+Shift+K`,
	},
	{
		feature: "Pin Tab",
		description: "Right-click any tab and select Pin — pinned tabs stay visible when you switch branches.",
		shortcut: null,
	},
	{
		feature: "Automation worktree cleanup",
		description:
			"Open Automations → Runs to inspect a completed run’s worktree and remove it when the backend confirms it is safe.",
		shortcut: null,
	},
	{
		feature: "Worktree Manager",
		description: "Create, delete, and merge worktrees in a unified panel.",
		shortcut: `${mod}+Shift+W`,
	},
	{
		feature: "Ideas Panel",
		description: "A per-repo scratchpad for context, links, and commands — click the Ideas icon in the toolbar.",
		shortcut: null,
	},
	{
		feature: "Detach to Window",
		description: "Right-click any tab and select Detach — it floats into its own OS window.",
		shortcut: null,
	},
	{
		feature: "Deep Links",
		description: "Use tuic://repo/path URLs in scripts or docs to open repos directly in TUICommander.",
		shortcut: null,
	},
	{
		feature: "Project Progress",
		description:
			"Open Terminal Progress from the toolbar bell to see what agents did while you were away, even when there are no unread updates.",
		shortcut: "Cmd+Shift+P",
	},
	{
		feature: "Plan Panel",
		description: "When an agent creates a plan file, the Plan panel opens automatically — track progress live.",
		shortcut: null,
	},
	{
		feature: "PTY Pause / Resume",
		description: "Freeze terminal output without killing the process — great for reading logs.",
		shortcut: `${mod}+Shift+Space`,
	},
	{
		feature: "MCP Proxy",
		description: "Go to Settings → MCP to add upstream MCP servers — all connected agents can use them.",
		shortcut: null,
	},
	{
		feature: "Kitty Keyboard Protocol",
		description:
			"Always on: agents that request it, such as Claude Code, get full Shift+Enter, Ctrl+Backspace, and other modifier combos. There is nothing to enable.",
		shortcut: null,
	},
	{
		feature: "Voice Dictation",
		description: "Dictate into the active terminal — runs locally with Whisper, no network needed.",
		shortcut: null,
	},
	{
		feature: "Find in Content",
		description:
			"Search text in terminals and markdown viewers — matches highlight as you type, with case, regex, and whole-word toggles.",
		shortcut: `${mod}+F`,
	},
	{
		feature: "Clickable File Paths",
		description: "Click any file path in terminal output to open a diff or preview panel instantly.",
		shortcut: null,
	},
	{
		feature: "Git Panel",
		description: "Stage, commit, browse history, and view blame — all in a tabbed side panel.",
		shortcut: `${mod}+Shift+D`,
	},
	{
		feature: "Commit Graph",
		description:
			"The Log tab in the Git Panel shows a visual commit graph with color-coded lanes and branch connections.",
		shortcut: null,
	},
	{
		feature: "Blame Heatmap",
		description:
			"The Blame sub-panel shows an age heatmap — recent changes are highlighted in green, older changes fade to neutral.",
		shortcut: null,
	},
	{
		feature: "Tab Switching",
		description: "Jump to any of the first 9 tabs by number — no mouse needed.",
		shortcut: `${mod}+1–9`,
	},
	{
		feature: "Help Panel",
		description: "See all keyboard shortcuts, version info, and about details in one place.",
		shortcut: `${mod}+?`,
	},
	{
		feature: "IDE Launcher",
		description: "Open the current repo in VS Code, Cursor, or your preferred editor from the toolbar.",
		shortcut: null,
	},
	{
		feature: "PR Monitoring",
		description: "CI check rings appear on branches in the sidebar — green, yellow, red at a glance.",
		shortcut: null,
	},
	{
		feature: "Session-Aware Resume",
		description: "After restart, agent tabs show a clickable banner to resume where you left off.",
		shortcut: null,
	},
	{
		feature: "Diff Panel",
		description: "Click any changed file in the sidebar to open a side-by-side diff view.",
		shortcut: null,
	},
	{
		feature: "Park Repos",
		description: "Right-click a repo header and select Park to hide inactive repos from the sidebar.",
		shortcut: null,
	},
	{
		feature: "Git Quick Actions",
		description: "Commit, push, pull, and fetch directly from the sidebar — no terminal needed.",
		shortcut: null,
	},
	{
		feature: "Claude Usage Dashboard",
		description: "Track API costs, token usage, and session history across all your Claude sessions.",
		shortcut: null,
	},
	{
		feature: "Custom Keybindings",
		description: "Rebind any shortcut from Help → Keyboard Shortcuts — click the pencil icon next to a key.",
		shortcut: null,
	},
	{
		feature: "Move Terminal to Worktree",
		description:
			"Right-click a terminal tab → Move to Worktree to move it to a different worktree. Also available via Command Palette.",
		shortcut: null,
	},
	{
		feature: "Post-Merge Cleanup",
		description:
			"Before removing a worktree, review its branch history, uncommitted files, and live terminal sessions in the confirmation dialog. Cleanup can interrupt an agent working there.",
		shortcut: null,
	},
	{
		feature: "Unseen Terminal Dot",
		description:
			"When a terminal finishes work while you're viewing another tab, its dot turns purple. Click it to clear the unseen state. Sidebar branches also turn purple if they contain unseen terminals.",
		shortcut: null,
	},
	{
		feature: "PR Actions",
		description:
			"Click a PR badge to open the popover — Merge, Approve, and View Diff are right there. After merging, a cleanup dialog handles branch deletion for you.",
		shortcut: null,
	},
	{
		feature: "PR Diff Tab",
		description: "Click View Diff in any PR popover to open a dedicated diff panel with collapsible file sections.",
		shortcut: null,
	},
	{
		feature: "Dismiss Remote PRs",
		description:
			"Remote-only PRs cluttering your sidebar? Dismiss them — use Show Dismissed to bring them back when needed.",
		shortcut: null,
	},
	{
		feature: "Status Bar Info",
		description:
			"Hover the status info text for a tooltip balloon with the full message — no more guessing truncated text.",
		shortcut: null,
	},
	{
		feature: "File Drag & Drop",
		description:
			"Drag files from Finder onto the terminal area to open them — markdown in the viewer, code in the editor.",
		shortcut: null,
	},
	{
		feature: "Image Paste in Ideas",
		description:
			"Paste images into the Ideas panel with Ctrl+V — thumbnails render inline, and paths are sent to agents so they can read the files.",
		shortcut: null,
	},
	{
		feature: "Branch Management",
		description:
			"Press Cmd+G to open the Branches tab — checkout, create, delete, merge, rebase, push, and pull without touching the terminal.",
		shortcut: `${mod}+G`,
	},
	{
		feature: "Prefix Folding",
		description:
			"In the Branches tab, branches sharing a common prefix (feature/, bugfix/) are automatically grouped. Toggle folding in the panel header.",
		shortcut: null,
	},
	{
		feature: "Branch Context Menu",
		description:
			"Right-click any branch in the Branches tab for quick actions: merge, rebase, compare diff, push, delete.",
		shortcut: null,
	},
	{
		feature: "GitHub Login",
		description:
			"Sign in to GitHub from Settings > Git & GitHub for automatic PR and CI monitoring. No need to manage tokens manually.",
		shortcut: null,
	},
	{
		feature: "GitHub Ops Proposals",
		description:
			"Open the GitHub Ops dashboard to scan a repo for refactor, testing, or performance proposals, then create issues only for the ones you approve.",
		shortcut: null,
	},
	{
		feature: "MCP Per-Repo",
		description:
			"Choose which upstream MCP servers are active for each repo. Toggle servers on/off per-repo, or share the list via .tuic.json.",
		shortcut: `${mod}+Shift+M`,
	},
	{
		feature: "Smart Prompts",
		description:
			"One-click AI automation for common git and code tasks — commit, review, create PRs, fix CI, and more.",
		shortcut: `${mod}+K`,
	},
	{
		feature: "Smart Prompts Toolbar",
		description:
			"Smart Prompts like Review Changes inject context-aware prompts into your agent. Find them in the toolbar lightning bolt menu.",
		shortcut: null,
	},
	{
		feature: "Custom Smart Prompts",
		description:
			"Customize Smart Prompts in Settings > Smart Prompts. You can edit built-in prompts or create your own.",
		shortcut: null,
	},
	{
		feature: "Split/Unified Diff",
		description:
			"Toggle between side-by-side and inline diff views with the toolbar buttons in any diff tab. Your preference is saved.",
		shortcut: null,
	},
	{
		feature: "Line-Level Restore",
		description:
			"Click individual lines in a diff to select them, then discard or unstage just those lines. Shift+click for range selection.",
		shortcut: null,
	},
	{
		feature: "Zoom All Terminals",
		description:
			"Increase, decrease, or reset font size across every terminal at once — no need to adjust each one individually.",
		shortcut: `${mod}+Shift+= / ${mod}+Shift+- / ${mod}+Shift+0`,
	},
	{
		feature: "Code Editor Zoom",
		description: "Zoom the open code editor without changing your terminal or another editor tab.",
		shortcut: `${mod}+= / ${mod}+- / ${mod}+0`,
	},
	{
		feature: "Global Workspace",
		description:
			"Promote terminals from different repos into a single cross-repo view. Right-click a tab → Promote, then toggle the workspace with the shortcut. Tabs show a globe icon when promoted.",
		shortcut: `${mod}+Shift+X`,
	},
	{
		feature: "GitHub Issues Panel",
		description:
			"Browse, filter, and act on GitHub issues alongside PRs — all in the unified GitHub panel. Smart prompts work on issues too.",
		shortcut: null,
	},
	{
		feature: "Interactive Checkboxes",
		description:
			"Task-list checkboxes in markdown viewers are clickable — cycle through unchecked, checked, and indeterminate states with a click.",
		shortcut: null,
	},
	{
		feature: "Sidebar Stats Badge",
		description: "Click the file-count badge next to a branch name to jump straight to the Git Changes panel.",
		shortcut: null,
	},
	{
		feature: "Clickable Palette Hints",
		description:
			"The footer mode hints in the command palette (! for files, ? for content) are now clickable — tap one to switch mode instantly.",
		shortcut: null,
	},
	{
		feature: "Repo Overlay on Tab Hover",
		description:
			"In the Global Workspace, hover a tab to see which repo it belongs to — useful when terminals from multiple repos share the view.",
		shortcut: null,
	},
	{
		feature: "WSL Support",
		description:
			"TUICommander runs natively on Windows with WSL — shell arguments, environment, and CWD paths are translated automatically.",
		shortcut: null,
	},
	{
		feature: "Focus Mode",
		description:
			"Hide the sidebar, tab bar, and side panels to maximize the active tab. Toolbar and status bar stay visible. Press again to restore.",
		shortcut: `${mod}+Alt+Enter`,
	},
	{
		feature: "tuic CLI",
		description:
			"Control TUICommander from the terminal. Open files, manage sessions, spawn agents, and use it as a tmux replacement. Install from Settings > General.",
		shortcut: null,
	},
	{
		feature: "Managed Agent Workspace Trust",
		description:
			"Agents spawned by another agent can start in a new folder without a trust question. Change this for Claude or direct Codex in Settings > Agents.",
		shortcut: null,
	},
	{
		feature: "tuic as tmux",
		description:
			"Run 'tuic alias' to create a tmux compatibility symlink. Any tool expecting tmux (like Claude Code) will use TUICommander instead.",
		shortcut: null,
	},
	{
		feature: "tuic open",
		description:
			"Open files in TUICommander from the terminal: 'tuic file.rs:42' opens at line 42. 'tuic .' opens the current directory as a repo.",
		shortcut: null,
	},
	{
		feature: "SSH Tunnel Auto-Connect",
		description:
			"Enable Auto-Connect on a tunnel profile to have it start automatically when TUICommander launches — no manual clicks needed.",
		shortcut: null,
	},
	{
		feature: "Remote Machine Deploy",
		description:
			"Choose Deploy on connect in Settings > Remote Machines to stage and launch the matching daemon over SSH automatically.",
		shortcut: null,
	},
	{
		feature: "Toggle Tunnels Panel",
		description:
			"Open the command palette and type 'tunnels' to toggle the SSH Tunnels panel without navigating to Settings.",
		shortcut: `${mod}+P`,
	},
	{
		feature: "Fold Command Blocks",
		description:
			"Collapse long command output to a single line. Toggle fold on the current block, or fold all blocks at once from the block settings.",
		shortcut: `${mod}+Shift+.`,
	},
	{
		feature: "Block-Scoped Search",
		description: "Toggle block-scoped search to restrict find-in-terminal to only the current command block's output.",
		shortcut: `${mod}+Shift+B`,
	},
	{
		feature: "Block Timestamps",
		description: "Hold Ctrl+Cmd to reveal when each command block started, shown as relative time in the gutter.",
		shortcut: null,
	},
	{
		feature: "Navigate Between Blocks",
		description: "Jump between command block boundaries to quickly hop through your terminal history.",
		shortcut: `${mod}+Shift+Up/Down`,
	},
	{
		feature: "Secure Generators",
		description:
			"Generate passwords, UUIDs, JWTs, TOTP secrets, and more from the command palette. All values are generated with cryptographic randomness.",
		shortcut: null,
	},
	{
		feature: "Native Agent Hooks",
		description:
			"Let Claude or Gemini report status through their own hooks for precise busy/idle/waiting state — enable per agent in Settings → Agents. TUIC installs and removes the hooks cleanly and never touches your own.",
		shortcut: null,
	},
	{
		feature: "Automation Templates",
		description:
			"Choose a built-in template in a new automation draft after entering its repository and run config. Review it and save it paused before enabling runs.",
		shortcut: null,
	},
];
