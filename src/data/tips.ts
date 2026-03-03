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
    description: `Press ${mod}+Shift+P to search and run any action by name — shortcuts, settings, panels, everything.`,
    shortcut: `${mod}+Shift+P`,
  },
  {
    feature: "Activity Dashboard",
    description: `Press ${mod}+K to see all agent sessions, CPU usage, and errors in one panel.`,
    shortcut: `${mod}+K`,
  },
  {
    feature: "Split Panes",
    description: `Press ${mod}+\\ to split the current terminal into two side-by-side panes.`,
    shortcut: `${mod}+\\`,
  },
  {
    feature: "Quick Branch Switch",
    description: `Press ${mod}+B to fuzzy-search all branches and switch instantly.`,
    shortcut: `${mod}+B`,
  },
  {
    feature: "Reopen Closed Tab",
    description: `Press ${mod}+Shift+T to bring back the last tab you closed.`,
    shortcut: `${mod}+Shift+T`,
  },
  {
    feature: "Terminal Zoom",
    description: `Press ${mod}+= to full-screen the active terminal, hiding all sidebar and toolbar chrome.`,
    shortcut: `${mod}+=`,
  },
  {
    feature: "File Browser",
    description: `Press ${mod}+E to browse repo files in a side panel with live diff preview.`,
    shortcut: `${mod}+E`,
  },
  {
    feature: "Prompt Library",
    description: `Press ${mod}+L to open reusable prompt templates — paste them into any agent session.`,
    shortcut: `${mod}+L`,
  },
  {
    feature: "Pin Tab",
    description: "Right-click any tab and select Pin — pinned tabs stay visible when you switch branches.",
    shortcut: null,
  },
  {
    feature: "Worktree Manager",
    description: `Press ${mod}+Shift+W to open the worktree panel — create, delete, and merge worktrees.`,
    shortcut: `${mod}+Shift+W`,
  },
  {
    feature: "Notes Panel",
    description: "Click the Notes icon in the toolbar to open a per-repo scratchpad for context, links, and commands.",
    shortcut: null,
  },
  {
    feature: "Detach to Window",
    description: "Right-click any tab and select Detach — it floats into its own OS window.",
    shortcut: null,
  },
  {
    feature: "Deep Links",
    description: "Use tuic://repo/path URLs in scripts or docs to open repos and files directly in TUICommander.",
    shortcut: null,
  },
  {
    feature: "Plan Panel",
    description: "When an agent creates a plan file, the Plan panel opens automatically — track progress live.",
    shortcut: null,
  },
  {
    feature: "PTY Pause / Resume",
    description: `Press ${mod}+Shift+Space to freeze terminal output without killing the process — great for reading logs.`,
    shortcut: `${mod}+Shift+Space`,
  },
  {
    feature: "MCP Proxy",
    description: "Go to Settings → Services to add upstream MCP servers — all connected agents can use them.",
    shortcut: null,
  },
  {
    feature: "Kitty Keyboard Protocol",
    description: "Enable in Settings → Agents to get full Shift+Enter, Ctrl+Alt, and other modifier combos in terminals.",
    shortcut: null,
  },
  {
    feature: "Voice Dictation",
    description: `Press ${mod}+Shift+D to dictate into the active terminal — runs locally with Whisper, no network needed.`,
    shortcut: `${mod}+Shift+D`,
  },
];
