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
    shortcut: `${mod}+Shift+P`,
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
    feature: "Prompt Library",
    description: "Reusable prompt templates — paste them into any agent session.",
    shortcut: `${mod}+L`,
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
    shortcut: `${mod}+Shift+D`,
  },
  {
    feature: "Find in Terminal",
    description: "Search text in the terminal scrollback buffer — matches highlight as you type.",
    shortcut: `${mod}+F`,
  },
  {
    feature: "Clickable File Paths",
    description: "Click any file path in terminal output to open a diff or preview panel instantly.",
    shortcut: null,
  },
  {
    feature: "Lazygit",
    description: "Open Lazygit in a dedicated tab for full-featured git operations.",
    shortcut: `${mod}+G`,
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
];
