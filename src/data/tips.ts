export interface Tip {
  feature: string;
  description: string;
  shortcut: string | null;
}

const isMac =
  typeof navigator !== "undefined"
    ? /Mac|iPhone|iPad|iPod/.test(navigator.platform)
    : true;

const mod = isMac ? "Cmd" : "Ctrl";

export const TIPS: Tip[] = [
  {
    feature: "Command Palette",
    description: "Search and run any action by name — the fastest way to navigate.",
    shortcut: `${mod}+Shift+P`,
  },
  {
    feature: "Activity Dashboard",
    description: "Monitor all agent sessions, CPU usage, and errors in one view.",
    shortcut: `${mod}+K`,
  },
  {
    feature: "Split Panes",
    description: "Open two terminals side-by-side in the same tab group.",
    shortcut: `${mod}+\\`,
  },
  {
    feature: "Quick Branch Switch",
    description: "Fuzzy-search all branches and switch instantly.",
    shortcut: `${mod}+B`,
  },
  {
    feature: "Reopen Closed Tab",
    description: "Accidentally closed a tab? Bring it back.",
    shortcut: `${mod}+Shift+T`,
  },
  {
    feature: "Terminal Zoom",
    description: "Full-screen the active terminal, hiding all chrome.",
    shortcut: `${mod}+=`,
  },
  {
    feature: "File Browser",
    description: "Browse repo files with live diff preview in the side panel.",
    shortcut: `${mod}+E`,
  },
  {
    feature: "Prompt Library",
    description: "Save and reuse prompt templates across any agent session.",
    shortcut: `${mod}+L`,
  },
  {
    feature: "Pin Tab",
    description: "Pin a tab so it stays visible when you switch branches.",
    shortcut: null,
  },
  {
    feature: "Worktree Manager",
    description: "Open, delete, and merge worktrees in a unified panel.",
    shortcut: `${mod}+Shift+W`,
  },
  {
    feature: "Notes Panel",
    description: "Per-repo scratchpad — jot context, links, and commands.",
    shortcut: null,
  },
  {
    feature: "Detach to Window",
    description: "Float any tab into its own OS window via right-click.",
    shortcut: null,
  },
  {
    feature: "Deep Links",
    description: "Open repos or files directly from scripts using tuic:// URLs.",
    shortcut: null,
  },
  {
    feature: "Plan Panel",
    description: "Auto-detects agent plan files and renders them as a live document.",
    shortcut: null,
  },
  {
    feature: "PTY Pause / Resume",
    description: "Freeze terminal output without killing the process — great for reading logs.",
    shortcut: `${mod}+Shift+Space`,
  },
  {
    feature: "MCP Proxy",
    description: "Route any MCP server through TUICommander for all connected agents.",
    shortcut: null,
  },
  {
    feature: "Kitty Keyboard Protocol",
    description: "Full modifier key support (Shift+Enter, Ctrl+Alt, etc.) in supported terminals.",
    shortcut: null,
  },
  {
    feature: "Voice Dictation",
    description: "Dictate into any input using Whisper — no network required.",
    shortcut: `${mod}+Shift+D`,
  },
];
