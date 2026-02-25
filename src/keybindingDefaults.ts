/**
 * Canonical keybinding action names and their default key combos.
 *
 * Key combo format:
 * - "Cmd" = platform-agnostic primary modifier (Meta on macOS, Ctrl on Win/Linux)
 * - "Ctrl" = literal Control key (for Cmd+Ctrl combos on macOS)
 * - "Shift", "Alt" = standard modifiers
 * - Key name after last "+" is the actual key (lowercase in normalized form)
 *
 * Users override these via keybindings.json — only overrides need to be listed.
 */

export const ACTION_NAMES = [
  "zoom-in",
  "zoom-out",
  "zoom-reset",
  "new-terminal",
  "close-terminal",
  "split-vertical",
  "split-horizontal",
  "run-command",
  "edit-command",
  "toggle-diff",
  "toggle-markdown",
  "toggle-notes",
  "toggle-file-browser",
  "toggle-prompt-library",
  "toggle-settings",
  "toggle-task-queue",
  "reopen-closed-tab",
  "toggle-sidebar",
  "prev-tab",
  "next-tab",
  "clear-terminal",
  "open-lazygit",
  "toggle-git-ops",
  "toggle-help",
  "open-lazygit-pane",
  "find-in-terminal",
  "command-palette",
  "activity-dashboard",
  "toggle-error-log",
  // Numbered tabs and branches
  ...Array.from({ length: 9 }, (_, i) => `switch-tab-${i + 1}`),
  ...Array.from({ length: 9 }, (_, i) => `switch-branch-${i + 1}`),
] as const;

export type ActionName = (typeof ACTION_NAMES)[number];

/**
 * Normalize a key combo string for consistent lookup.
 * Sorts modifiers alphabetically, lowercases everything.
 * "Cmd+Shift+D" → "cmd+shift+d"
 * "Shift+Cmd+D" → "cmd+shift+d"
 */
export function normalizeCombo(combo: string): string {
  if (!combo) return "";
  const parts = combo.split("+");
  const key = parts.pop()!.toLowerCase();
  const modifiers = parts.map((m) => m.toLowerCase()).sort();
  return [...modifiers, key].join("+");
}

/** Default keybindings — "Cmd" is resolved to Meta/Ctrl at runtime */
export const DEFAULT_BINDINGS: Record<ActionName, string> = {
  "zoom-in": "Cmd+=",
  "zoom-out": "Cmd+-",
  "zoom-reset": "Cmd+0",
  "new-terminal": "Cmd+T",
  "close-terminal": "Cmd+W",
  "split-vertical": "Cmd+\\",
  "split-horizontal": "Cmd+Alt+\\",
  "run-command": "Cmd+R",
  "edit-command": "Cmd+Shift+R",
  "toggle-diff": "Cmd+Shift+D",
  "toggle-markdown": "Cmd+M",
  "toggle-notes": "Cmd+N",
  "toggle-file-browser": "Cmd+E",
  "toggle-prompt-library": "Cmd+K",
  "toggle-settings": "Cmd+,",
  "toggle-task-queue": "Cmd+J",
  "reopen-closed-tab": "Cmd+Shift+T",
  "toggle-sidebar": "Cmd+[",
  "prev-tab": "Cmd+Shift+[",
  "next-tab": "Cmd+Shift+]",
  "clear-terminal": "Cmd+L",
  "open-lazygit": "Cmd+G",
  "toggle-git-ops": "Cmd+Shift+G",
  "toggle-help": "Cmd+?",
  "open-lazygit-pane": "Cmd+Shift+L",
  "find-in-terminal": "Cmd+F",
  "command-palette": "Cmd+Shift+P",
  "activity-dashboard": "Cmd+Shift+A",
  "toggle-error-log": "Cmd+Shift+E",
  // Numbered tabs
  ...Object.fromEntries(
    Array.from({ length: 9 }, (_, i) => [`switch-tab-${i + 1}`, `Cmd+${i + 1}`]),
  ),
  // Numbered branches (Cmd+Ctrl on macOS, Ctrl+Alt on Win/Linux — we use Cmd+Ctrl here)
  ...Object.fromEntries(
    Array.from({ length: 9 }, (_, i) => [`switch-branch-${i + 1}`, `Cmd+Ctrl+${i + 1}`]),
  ),
} as Record<ActionName, string>;
