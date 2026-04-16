export interface Tip {
  feature: string;
  description: string;
  shortcut: string | null;
}

import { isMacOS } from "../platform";

const mod = isMacOS() ? "Cmd" : "Ctrl";

export const TIPS: Tip[] = [
  {
    feature: "Command Palette",
    description: "Search and run any action by name — shortcuts, settings, panels, everything.",
    shortcut: `${mod}+P`,
  },
  {
    feature: "Find Files",
    description: "Type ! in the command palette to search files by name. Type ? to search file contents. Results open in the editor.",
    shortcut: `${mod}+P then ! or ?`,
  },
  {
    feature: "Activity Dashboard",
    description: "See all agent sessions, CPU usage, and errors in one panel.",
    shortcut: `${mod}+K`,
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
    feature: "File Browser",
    description: "Browse repo files in a side panel with live diff preview.",
    shortcut: `${mod}+E`,
  },
  {
    feature: "File Content Search",
    description: "Press C in the file browser search bar to switch to content search — searches inside files with regex, case-sensitive, and whole-word options.",
    shortcut: `${mod}+Shift+F`,
  },
  {
    feature: "Prompts",
    description: "Press Cmd+Shift+K to open the Prompts dropdown — 24 built-in context-aware prompts for git, review, PR, and more.",
    shortcut: `${mod}+Shift+K`,
  },
  {
    feature: "Pin Tab",
    description: "Right-click any tab and select Pin — pinned tabs stay visible when you switch branches.",
    shortcut: null,
  },
  {
    feature: "Worktree Manager",
    description: "Create, delete, and merge worktrees in a unified panel.",
    shortcut: `${mod}+Shift+W`,
  },
  {
    feature: "Notes Panel",
    description: "A per-repo scratchpad for context, links, and commands — click the Notes icon in the toolbar.",
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
    description: "Go to Settings → Services to add upstream MCP servers — all connected agents can use them.",
    shortcut: null,
  },
  {
    feature: "Kitty Keyboard Protocol",
    description: "Enable in Settings → Agents for full Shift+Enter, Ctrl+Alt, and other modifier combos.",
    shortcut: null,
  },
  {
    feature: "Voice Dictation",
    description: "Dictate into the active terminal — runs locally with Whisper, no network needed.",
    shortcut: null,
  },
  {
    feature: "Find in Content",
    description: "Search text in terminals and markdown viewers — matches highlight as you type, with case, regex, and whole-word toggles.",
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
    description: "The Log tab in the Git Panel shows a visual commit graph with color-coded lanes and branch connections.",
    shortcut: null,
  },
  {
    feature: "Blame Heatmap",
    description: "The Blame sub-panel shows an age heatmap — recent changes are highlighted in green, older changes fade to neutral.",
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
    description: "Right-click a terminal tab → Move to Worktree to move it to a different worktree. Also available via Command Palette.",
    shortcut: null,
  },
  {
    feature: "Post-Merge Cleanup",
    description: "After merging a PR or worktree branch, a cleanup dialog lets you archive/delete the worktree, switch branch, pull, and delete the merged branch — all via backend, even while an agent runs in the terminal.",
    shortcut: null,
  },
  {
    feature: "Unseen Terminal Dot",
    description: "When a terminal finishes work while you're viewing another tab, its dot turns purple. Click it to clear the unseen state. Sidebar branches also turn purple if they contain unseen terminals.",
    shortcut: null,
  },
  {
    feature: "PR Actions",
    description: "Click a PR badge to open the popover — Merge, Approve, and View Diff are right there. After merging, a cleanup dialog handles branch deletion for you.",
    shortcut: null,
  },
  {
    feature: "PR Diff Tab",
    description: "Click View Diff in any PR popover to open a dedicated diff panel with collapsible file sections.",
    shortcut: null,
  },
  {
    feature: "Dismiss Remote PRs",
    description: "Remote-only PRs cluttering your sidebar? Dismiss them — use Show Dismissed to bring them back when needed.",
    shortcut: null,
  },
  {
    feature: "Status Bar Info",
    description: "Hover the status info text for a tooltip balloon with the full message — no more guessing truncated text.",
    shortcut: null,
  },
  {
    feature: "File Drag & Drop",
    description: "Drag files from Finder onto the terminal area to open them — markdown in the viewer, code in the editor.",
    shortcut: null,
  },
  {
    feature: "Image Paste in Notes",
    description: "Paste images into the Notes panel with Ctrl+V — thumbnails render inline, and paths are sent to agents so they can read the files.",
    shortcut: null,
  },
  {
    feature: "Branch Management",
    description: "Press Cmd+G to open the Branches tab — checkout, create, delete, merge, rebase, push, and pull without touching the terminal.",
    shortcut: `${mod}+G`,
  },
  {
    feature: "Prefix Folding",
    description: "In the Branches tab, branches sharing a common prefix (feature/, bugfix/) are automatically grouped. Toggle folding in the panel header.",
    shortcut: null,
  },
  {
    feature: "Branch Context Menu",
    description: "Right-click any branch in the Branches tab for quick actions: merge, rebase, compare diff, push, delete.",
    shortcut: null,
  },
  {
    feature: "GitHub Login",
    description: "Sign in to GitHub from Settings > GitHub for automatic PR and CI monitoring. No need to manage tokens manually.",
    shortcut: null,
  },
  {
    feature: "MCP Per-Repo",
    description: "Choose which upstream MCP servers are active for each repo. Toggle servers on/off per-repo, or share the list via .tuic.json.",
    shortcut: `${mod}+Shift+M`,
  },
  {
    feature: "Smart Prompts",
    description: "One-click AI automation for common git and code tasks — commit, review, create PRs, fix CI, and more.",
    shortcut: `${mod}+K`,
  },
  {
    feature: "Smart Prompts Toolbar",
    description: "Smart Prompts like Review Changes inject context-aware prompts into your agent. Find them in the toolbar lightning bolt menu.",
    shortcut: null,
  },
  {
    feature: "Custom Smart Prompts",
    description: "Customize Smart Prompts in Settings > Smart Prompts. You can edit built-in prompts or create your own.",
    shortcut: null,
  },
  {
    feature: "Split/Unified Diff",
    description: "Toggle between side-by-side and inline diff views with the toolbar buttons in any diff tab. Your preference is saved.",
    shortcut: null,
  },
  {
    feature: "Line-Level Restore",
    description: "Click individual lines in a diff to select them, then discard or unstage just those lines. Shift+click for range selection.",
    shortcut: null,
  },
  {
    feature: "Zoom All Terminals",
    description: "Increase, decrease, or reset font size across every terminal at once — no need to adjust each one individually.",
    shortcut: `${mod}+Shift+= / ${mod}+Shift+- / ${mod}+Shift+0`,
  },
  {
    feature: "Global Workspace",
    description: "Promote terminals from different repos into a single cross-repo view. Right-click a tab → Promote, then toggle the workspace with the shortcut. Tabs show a globe icon when promoted.",
    shortcut: `${mod}+Shift+X`,
  },
  {
    feature: "GitHub Issues Panel",
    description: "Browse, filter, and act on GitHub issues alongside PRs — all in the unified GitHub panel. Smart prompts work on issues too.",
    shortcut: null,
  },
  {
    feature: "Interactive Checkboxes",
    description: "Task-list checkboxes in markdown viewers are clickable — cycle through unchecked, checked, and indeterminate states with a click.",
    shortcut: null,
  },
  {
    feature: "Sidebar Stats Badge",
    description: "Click the file-count badge next to a branch name to jump straight to the Git Changes panel.",
    shortcut: null,
  },
  {
    feature: "Clickable Palette Hints",
    description: "The footer mode hints in the command palette (! for files, ? for content) are now clickable — tap one to switch mode instantly.",
    shortcut: null,
  },
  {
    feature: "Repo Overlay on Tab Hover",
    description: "In the Global Workspace, hover a tab to see which repo it belongs to — useful when terminals from multiple repos share the view.",
    shortcut: null,
  },
  {
    feature: "WSL Support",
    description: "TUICommander runs natively on Windows with WSL — shell arguments, environment, and CWD paths are translated automatically.",
    shortcut: null,
  },
  {
    feature: "Focus Mode",
    description: "Hide the sidebar, tab bar, and side panels to maximize the active tab. Toolbar and status bar stay visible. Press again to restore.",
    shortcut: `${mod}+Alt+Enter`,
  },
];
